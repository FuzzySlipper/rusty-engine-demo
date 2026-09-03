using System.Numerics;
using Rusty.Engine;

namespace LoadingBay.Game;

/// <summary>
/// Product-owned E1M1 semantics projected through the generated Perception,
/// Presentation, Animation, and Audio boundaries. Engine retains realization.
/// </summary>
internal sealed class LoadingBayWorldServices : IDisposable
{
    private readonly LoadingBayPerceptionProjection _perception;
    private readonly LoadingBayExitPresentation _presentation;
    private readonly LoadingBayExitButtonAnimation _animation;
    private readonly LoadingBayAudioPolicy _audio;
    private bool _observationPending;
    private bool _sharedRealizationsActive;
    private bool _disposed;

    internal LoadingBayWorldServices(
        IEngineContext engine,
        LoadingBayTuning tuning,
        LoadingBayExitPresentation exitPresentation,
        LoadingBayExitButtonAnimation exitButtonAnimation)
    {
        ArgumentNullException.ThrowIfNull(exitPresentation);
        ArgumentNullException.ThrowIfNull(exitButtonAnimation);
        List<IDisposable> constructed = [];
        try
        {
            _perception = new LoadingBayPerceptionProjection(engine.Perception, tuning);
            _presentation = exitPresentation;
            _animation = exitButtonAnimation;
            _audio = new LoadingBayAudioPolicy(engine.Audio, tuning);
        }
        catch (Exception constructionFailure)
        {
            List<Exception> failures = [constructionFailure];
            for (int index = constructed.Count - 1; index >= 0; index--)
            {
                try { constructed[index].Dispose(); }
                catch (Exception cleanupFailure) { failures.Add(cleanupFailure); }
            }
            throw failures.Count == 1 ? constructionFailure : new AggregateException(failures);
        }
    }

    internal void Update(ProductUpdate update)
    {
        ThrowIfDisposed();
        if (HasAdmittedSteps(update.Facts))
            _observationPending = true;
    }

    internal LoadingBayEngineServiceReadout Publish(LoadingBayReadout readout, LoadingBayPlayerScene player)
    {
        ThrowIfDisposed();
        if (_observationPending)
        {
            _perception.Observe(readout.Player.Value, player);
            _observationPending = false;
        }

        LoadingBayPerceptionReadout perception = _perception.Readout;
        if (_sharedRealizationsActive)
        {
            _presentation.Update(perception);
            _animation.Publish(readout.Complete);
        }
        return new LoadingBayEngineServiceReadout(perception, _presentation.Readout, _animation.Readout, _audio.Readout);
    }

    internal void ActivateSharedRealizations()
    {
        ThrowIfDisposed();
        _sharedRealizationsActive = true;
    }

    internal void DeactivateSharedRealizations()
    {
        ThrowIfDisposed();
        _sharedRealizationsActive = false;
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        List<Exception>? failures = null;
        // The authored landmark billboard and animation span gameplay generations
        // and are retired by LoadingBayProduct after the active session's HUD and
        // other Engine services are gone.
        if (failures is { Count: > 0 }) throw new AggregateException(failures);
    }

    private static bool HasAdmittedSteps(ProductUpdateFacts facts) => facts.AdmittedStepCount > 0
        && double.IsFinite(facts.FixedDeltaSeconds)
        && facts.FixedDeltaSeconds > 0d
        && facts.FixedDeltaSeconds <= float.MaxValue;

    private void ThrowIfDisposed()
    {
        if (_disposed) throw new ObjectDisposedException(nameof(LoadingBayWorldServices));
    }
}

/// <summary>
/// One retained Engine animated-mesh appearance for the authored E1M1 exit button.
/// The direct primary path packs the committed external texture closure; no clip pack,
/// rig, controller, local playback loop, or source parsing belongs to Loading Bay.
/// </summary>
internal sealed class LoadingBayExitButtonAnimation : IDisposable
{
    private readonly IAppearanceService _appearanceService;
    private readonly IAnimationService _animation;
    private readonly LoadingBayE1M1AnimationCue _cue;
    private readonly Appearance _appearance;
    private readonly AnimationInstance _instance;
    private LoadingBayAnimationReadout _readout;
    private bool _completionObserved;
    private bool _disposed;

    internal LoadingBayExitButtonAnimation(IAppearanceService appearanceService, IAnimationService animation, LoadingBayE1M1AnimationCue cue)
    {
        _appearanceService = appearanceService ?? throw new ArgumentNullException(nameof(appearanceService));
        _animation = animation ?? throw new ArgumentNullException(nameof(animation));
        _cue = cue;
        ValidateCue(cue);

        Appearance? appearance = null;
        AnimationInstance? instance = null;
        try
        {
            RenderResourceHandle mesh = _animation.OpenAnimatedMesh(new AnimatedMeshResourceRequest(cue.SourcePath));
            if (mesh.Value == 0) throw new InvalidOperationException("Engine Animation returned an invalid E1M1 exit-button mesh handle.");
            Appearance createdAppearance = _animation.CreateAnimatedMeshAppearance(new AnimatedMeshAppearanceRequest(mesh));
            AnimationInstance createdInstance = _animation.CreateInstance(new AnimationInstanceRequest(createdAppearance, cue.ObjectId));
            appearance = createdAppearance;
            instance = createdInstance;
            _appearance = createdAppearance;
            _instance = createdInstance;
            _appearanceService.PublishSnapshot(new[] { AppearanceFact() });
            _animation.SetPlayback(SampleOff());
            _readout = Read(false, 0);
            ValidateReadout(_readout);
        }
        catch
        {
            instance?.Dispose();
            appearance?.Dispose();
            throw;
        }
    }

    internal LoadingBayAnimationReadout Readout => _readout;

    internal void Publish(bool complete)
    {
        if (_disposed) throw new ObjectDisposedException(nameof(LoadingBayExitButtonAnimation));
        _appearanceService.PublishSnapshot(new[] { AppearanceFact() });
        if (complete && !_completionObserved)
        {
            _animation.SetPlayback(PlayOn());
            _completionObserved = true;
            _readout = Read(true, checked(_readout.CompletionTransitionCount + 1));
        }
        else
        {
            _readout = Read(_completionObserved, _readout.CompletionTransitionCount);
        }
        ValidateReadout(_readout);
    }

    /// <summary>Starts a newly committed gameplay generation from the authored off sample.</summary>
    internal LoadingBayExitButtonAnimationCheckpoint ResetForGeneration()
    {
        if (_disposed) throw new ObjectDisposedException(nameof(LoadingBayExitButtonAnimation));
        LoadingBayExitButtonAnimationCheckpoint checkpoint = new(_completionObserved, _readout);
        // Keep local transition state intact until Engine accepts the sample. If the
        // call fails, Restart retains the previous authoritative generation and its
        // observable animation state rather than claiming a reset that never landed.
        _animation.SetPlayback(SampleOff());
        LoadingBayAnimationReadout reset = Read(false, 0);
        ValidateReadout(reset);
        _completionObserved = false;
        _readout = reset;
        return checkpoint;
    }

    /// <summary>Restores the prior generation's observable cue after a failed commit publication.</summary>
    internal void Restore(LoadingBayExitButtonAnimationCheckpoint checkpoint)
    {
        if (_disposed) throw new ObjectDisposedException(nameof(LoadingBayExitButtonAnimation));
        _animation.SetPlayback(checkpoint.CompletionObserved ? PlayOn() : SampleOff());
        LoadingBayAnimationReadout restored = Read(checkpoint.CompletionObserved, checkpoint.Readout.CompletionTransitionCount);
        ValidateReadout(restored);
        _completionObserved = checkpoint.CompletionObserved;
        _readout = restored;
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        List<Exception>? failures = null;
        try { _instance.Dispose(); }
        catch (Exception exception) { failures = [exception]; }
        try { _appearanceService.PublishSnapshot(ReadOnlySpan<AppearanceFact>.Empty); }
        catch (Exception exception) { (failures ??= []).Add(exception); }
        try { _appearance.Dispose(); }
        catch (Exception exception) { (failures ??= []).Add(exception); }
        if (failures is { Count: > 0 }) throw new AggregateException(failures);
    }

    private AppearanceFact AppearanceFact() => new(_cue.ObjectId, _cue.Transform, _appearance, true, RenderLayer.Scene);

    private AnimationPlaybackRequest SampleOff() => new(
        _instance, AnimationPlaybackKind.Sample, _cue.OffClip, AnimationLoopMode.Once,
        _cue.PlaybackSpeed, _cue.PlaybackWeight, false, 0f, false, _cue.OffNormalizedTime);

    private AnimationPlaybackRequest PlayOn() => new(
        _instance, AnimationPlaybackKind.Play, _cue.OnClip, AnimationLoopMode.Once,
        _cue.PlaybackSpeed, _cue.PlaybackWeight, true, 0f, false, 0f);

    private LoadingBayAnimationReadout Read(bool completionObserved, uint transitionCount)
    {
        AnimationReadout engine = _animation.Read();
        AnimationRealizationReadout realization = _animation.ReadRealization();
        return new LoadingBayAnimationReadout(
            _cue.Id, true, completionObserved, transitionCount, engine.AdmittedMeshes,
            engine.RetainedInstances, engine.PendingPlaybackCommands,
            realization.RetainedFactCount, realization.EvictedFactCount);
    }

    private static void ValidateCue(LoadingBayE1M1AnimationCue cue)
    {
        if (string.IsNullOrWhiteSpace(cue.Id) || cue.ObjectId == 0 || string.IsNullOrWhiteSpace(cue.SourcePath) ||
            string.IsNullOrWhiteSpace(cue.OffClip) || string.IsNullOrWhiteSpace(cue.OnClip) || cue.OffClip == cue.OnClip ||
            !float.IsFinite(cue.PlaybackSpeed) || cue.PlaybackSpeed <= 0f ||
            !float.IsFinite(cue.PlaybackWeight) || cue.PlaybackWeight <= 0f ||
            !float.IsFinite(cue.OffNormalizedTime) || cue.OffNormalizedTime is < 0f or > 1f ||
            !float.IsFinite(cue.Transform.Translation.X) || !float.IsFinite(cue.Transform.Translation.Y) || !float.IsFinite(cue.Transform.Translation.Z) ||
            !float.IsFinite(cue.Transform.Scale.X) || !float.IsFinite(cue.Transform.Scale.Y) || !float.IsFinite(cue.Transform.Scale.Z))
            throw new InvalidOperationException("Loading Bay's E1M1 exit-button animation cue is invalid.");
    }

    private static void ValidateReadout(LoadingBayAnimationReadout readout)
    {
        if (!readout.RetainedAppearance || readout.AdmittedMeshes != 1 || readout.RetainedInstances != 1 ||
            (readout.CompletionObserved && readout.CompletionTransitionCount != 1) ||
            (!readout.CompletionObserved && readout.CompletionTransitionCount != 0))
            throw new InvalidOperationException("Engine Animation did not retain one coherent E1M1 exit-button realization.");
    }
}

/// <summary>One named authored E1M1 landmark visibility query; Engine owns the occlusion cast.</summary>
internal sealed class LoadingBayPerceptionProjection
{
    private readonly IPerceptionService _perception;
    private readonly LoadingBayTuning _tuning;
    private LoadingBayPerceptionReadout _readout;

    internal LoadingBayPerceptionProjection(IPerceptionService perception, LoadingBayTuning tuning)
    {
        _perception = perception ?? throw new ArgumentNullException(nameof(perception));
        _tuning = tuning;
        ValidateLandmark(tuning.ExitLandmark);
        _readout = new LoadingBayPerceptionReadout(tuning.ExitLandmark.Id, false, 0, 0, 0);
    }

    internal LoadingBayPerceptionReadout Readout => _readout;

    internal void Observe(ulong playerEntity, LoadingBayPlayerScene player)
    {
        PerceptionObserver observer = player.CreatePerceptionObserver(playerEntity, _tuning);
        LoadingBayE1M1Landmark landmark = _tuning.ExitLandmark;
        PerceptionReadoutLeaseReceipt receipt = _perception.QueryVisibility(new PerceptionQueryRequest(
            player.Session,
            new[] { observer },
            new[] { new PerceptionTarget(landmark.EntityId, landmark.Position) },
            ReadOnlyMemory<SpatialEntityCollider>.Empty,
            0,
            0,
            64));
        if (receipt.SelectedObservers != 1 || receipt.SelectedTargets != 1 || receipt.Pairs.Length != 1 || receipt.VisibilityCasts > 1)
            throw new InvalidOperationException("Engine Perception did not return one bounded E1M1 landmark visibility query.");

        PerceptionPair pair = receipt.Pairs.Span[0];
        if (pair.Observer != playerEntity || pair.Target != landmark.EntityId || !double.IsFinite(pair.Distance) || !double.IsFinite(pair.FacingCosine))
            throw new InvalidOperationException("Engine Perception returned an invalid E1M1 landmark visibility pair.");
        bool visible = pair.Kind == PerceptionPairKind.Visible;
        if (visible != _readout.Visible)
            _readout = new LoadingBayPerceptionReadout(landmark.Id, visible, checked(_readout.Revision + 1), receipt.VisibilityCasts, receipt.OcclusionRejects);
        else
            _readout = _readout with { VisibilityCasts = receipt.VisibilityCasts, OcclusionRejects = receipt.OcclusionRejects };
    }

    private static void ValidateLandmark(LoadingBayE1M1Landmark landmark)
    {
        if (string.IsNullOrWhiteSpace(landmark.Id) || landmark.EntityId == 0 ||
            !float.IsFinite(landmark.Position.X) || !float.IsFinite(landmark.Position.Y) || !float.IsFinite(landmark.Position.Z))
            throw new InvalidOperationException("Loading Bay's named E1M1 perception landmark is invalid.");
    }
}

/// <summary>Retained Engine structured billboard for the authored exit landmark; this is not a DOM HUD duplicate.</summary>
internal sealed class LoadingBayExitPresentation : IDisposable
{
    private static readonly Color Foreground = new(0.8f, 1f, 0.8f, 1f);
    private static readonly Color Background = new(0.02f, 0.08f, 0.02f, 0.85f);
    private static readonly Color Border = new(0.3f, 0.9f, 0.3f, 1f);
    private readonly IPresentationService _presentation;
    private readonly LoadingBayTuning _tuning;
    private readonly PresentationBillboard _billboard;
    private LoadingBayPerceptionReadout _observation;
    private PresentationFactsReadout _readout;
    private bool _disposed;

    internal LoadingBayExitPresentation(IPresentationService presentation, LoadingBayTuning tuning)
    {
        _presentation = presentation ?? throw new ArgumentNullException(nameof(presentation));
        _tuning = tuning;
        _observation = new LoadingBayPerceptionReadout(tuning.ExitLandmark.Id, false, 0, 0, 0);
        _billboard = _presentation.CreateStructuredBillboard(Descriptor(_observation));
        _readout = _presentation.Read();
        Validate(_readout);
    }

    internal PresentationFactsReadout Readout => _readout;

    internal void Update(LoadingBayPerceptionReadout observation)
    {
        if (_disposed) throw new ObjectDisposedException(nameof(LoadingBayExitPresentation));
        _presentation.UpdateStructuredBillboard(_billboard, Descriptor(observation));
        _readout = _presentation.Read();
        _observation = observation;
        Validate(_readout);
    }

    internal LoadingBayExitPresentationCheckpoint Checkpoint()
    {
        if (_disposed) throw new ObjectDisposedException(nameof(LoadingBayExitPresentation));
        return new LoadingBayExitPresentationCheckpoint(_observation, _readout);
    }

    internal void Restore(LoadingBayExitPresentationCheckpoint checkpoint)
    {
        if (_disposed) throw new ObjectDisposedException(nameof(LoadingBayExitPresentation));
        _presentation.UpdateStructuredBillboard(_billboard, Descriptor(checkpoint.Observation));
        PresentationFactsReadout restored = _presentation.Read();
        Validate(restored);
        _observation = checkpoint.Observation;
        _readout = checkpoint.Readout;
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        _billboard.Dispose();
    }

    private PresentationStructuredBillboardDescriptor Descriptor(LoadingBayPerceptionReadout observation)
    {
        LoadingBayE1M1Landmark landmark = _tuning.ExitLandmark;
        bool visible = observation.Visible;
        string status = visible ? "Exit visible" : "Exit occluded";
        PresentationBillboardMeter[] meters =
        [
            new PresentationBillboardMeter("exit-visibility", "loading-bay.exit.visibility", "Exit visibility", visible ? 1f : 0f, 0f, 1f, false, 0f,
                PresentationBillboardMeterFillDirection.LeftToRight, 1, Foreground, Foreground, Background, Border)
        ];
        PresentationBillboardStatusCue[] cues =
        [
            new PresentationBillboardStatusCue("exit-visibility", "loading-bay.exit.status", status, false, default)
        ];
        return new PresentationStructuredBillboardDescriptor(
            landmark.EntityId,
            new PresentationAnchor(PresentationAnchorKind.World, landmark.Position, 0, Vector3.Zero),
            true, "loading-bay.exit.label", landmark.Id,
            false, default,
            "loading-bay.exit.accessible", "E1M1 exit visibility",
            meters, cues,
            180f, 6f, PresentationBillboardAlignment.Center,
            new PresentationBillboardStyle(0.9f, Background, Border, 4f),
            new PresentationBillboardLayout(10, PresentationBillboardLayoutSizing.DistanceScaled, 24f, 0.75f, 1.5f,
                new PresentationBillboardSafeArea(8f, 8f, 8f, 8f), PresentationBillboardEdgeBehavior.Clamp, PresentationBillboardOverlapBehavior.Suppress),
            PresentationFontKind.System, default, "sans-serif", 16f, Foreground, Background, (float)_tuning.PerceptionMaximumDistance,
            PresentationBillboardLayer.Occluded, true);
    }

    private static void Validate(PresentationFactsReadout readout)
    {
        if (readout.ActiveBillboards == 0)
            throw new InvalidOperationException("Engine Presentation did not retain the E1M1 exit billboard.");
    }
}

/// <summary>Content policy has no E1M1 clips; retain only the real effects-bus state.</summary>
internal sealed class LoadingBayAudioPolicy
{
    private readonly IAudioService _audio;
    private readonly LoadingBayTuning _tuning;
    private AudioBusReadout _readout;

    internal LoadingBayAudioPolicy(IAudioService audio, LoadingBayTuning tuning)
    {
        _audio = audio ?? throw new ArgumentNullException(nameof(audio));
        _tuning = tuning;
        if (!float.IsFinite(tuning.EffectsVolume) || tuning.EffectsVolume is < 0f or > 1f)
            throw new InvalidOperationException("Loading Bay's effects-volume tuning must be finite and within [0, 1].");
        _audio.SetBusVolume(new AudioBusVolumeRequest(AudioBus.Sfx, tuning.EffectsVolume));
        _audio.SetBusMuted(new AudioBusMutedRequest(AudioBus.Sfx, tuning.EffectsMuted));
        _readout = _audio.ReadBus(new AudioBusReadRequest(AudioBus.Sfx));
        if (!float.IsFinite(_readout.Volume) || MathF.Abs(_readout.Volume - tuning.EffectsVolume) > 0.0001f || _readout.Muted != tuning.EffectsMuted)
            throw new InvalidOperationException("Engine Audio did not retain Loading Bay's E1M1 effects-bus policy.");
    }

    internal AudioBusReadout Readout => _readout;
}

internal readonly record struct LoadingBayPerceptionReadout(string LandmarkId, bool Visible, ulong Revision, uint VisibilityCasts, uint OcclusionRejects);
internal readonly record struct LoadingBayAnimationReadout(
    string CueId,
    bool RetainedAppearance,
    bool CompletionObserved,
    uint CompletionTransitionCount,
    uint AdmittedMeshes,
    uint RetainedInstances,
    uint PendingPlaybackCommands,
    uint RetainedRealizationFacts,
    ulong EvictedRealizationFacts);

/// <summary>Bounded product-local checkpoint used only around Restart's shared animation reset.</summary>
internal readonly record struct LoadingBayExitButtonAnimationCheckpoint(bool CompletionObserved, LoadingBayAnimationReadout Readout);
internal readonly record struct LoadingBayExitPresentationCheckpoint(LoadingBayPerceptionReadout Observation, PresentationFactsReadout Readout);
internal readonly record struct LoadingBayEngineServiceReadout(LoadingBayPerceptionReadout Perception, PresentationFactsReadout Presentation, LoadingBayAnimationReadout Animation, AudioBusReadout Audio);
