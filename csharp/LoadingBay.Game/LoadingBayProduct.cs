using System.Globalization;
using System.Numerics;
using Rusty.Engine;
using Rusty.Engine.Debugging;
using Rusty.Engine.Entities;

namespace LoadingBay.Game;

/// <summary>
/// Loading Bay's thin product lifecycle owner. The session owns E1M1 gameplay,
/// content admission, generated Engine service use, and UI projection.
/// </summary>
public sealed class LoadingBayProduct : IEngineProduct, IDebugCommandModuleSource
{
    private const int MaximumRetirementFailures = 8;
    private const string DebugWorldName = "loading-bay";
    private ILoadingBaySession? _session;
    private readonly Func<ILoadingBaySession> _sessionFactory;
    // The authored exit-button realization belongs to the product, rather than a
    // gameplay generation. Engine permits one instance for object 149, while a
    // Restart intentionally overlaps construction of the replacement and the
    // retirement of the prior generation.
    private readonly LoadingBayExitButtonAnimation? _exitButtonAnimation;
    private readonly LoadingBayExitPresentation? _exitPresentation;
    private readonly LoadingBaySkyBackground? _skyBackground;
    private readonly Queue<Exception> _retirementFailures = new();
    private ulong _droppedRetirementFailures;
    private bool _started;
    private bool _paused;
    private bool _shutdown;
    private readonly LoadingBayLiveDebugModule _liveDebug;
    private readonly EntityWorldDebugModule _entityWorldDebug;
    private bool _debugWorldRegistered;

    public LoadingBayProduct(ProductCreateContext context)
    {
        ArgumentNullException.ThrowIfNull(context);
        _entityWorldDebug = CreateEntityWorldDebugModule();
        _liveDebug = new LoadingBayLiveDebugModule(DebugReadout, DebugSetTrack);

        LoadingBayExitButtonAnimation? animation = null;
        LoadingBayExitPresentation? presentation = null;
        LoadingBaySkyBackground? skyBackground = null;
        ILoadingBaySession? session = null;
        try
        {
            skyBackground = new LoadingBaySkyBackground(
                context.Engine.Content,
                context.Content,
                context.Engine.Appearance,
                context.Engine.CameraView);
            _skyBackground = skyBackground;
            animation = new LoadingBayExitButtonAnimation(
                context.Engine.Appearance,
                context.Engine.Animation,
                LoadingBayTuning.E1M1.ExitButtonCue);
            _exitButtonAnimation = animation;
            presentation = new LoadingBayExitPresentation(context.Engine.Presentation, LoadingBayTuning.E1M1);
            _exitPresentation = presentation;
            _sessionFactory = () => CreateSession(context, presentation, animation, skyBackground);
            session = _sessionFactory();
            session.ActivateSharedRealizations();
            session.Publish();
            _session = session;
            AdoptDebugWorld(session);
        }
        catch (Exception constructionFailure)
        {
            Console.Error.WriteLine($"Loading Bay product construction failed: {constructionFailure}");
            List<Exception>? failures = null;
            try { DetachDebugWorld(session); }
            catch (Exception cleanupFailure) { (failures ??= []).Add(cleanupFailure); }
            try { UnregisterDebugWorld(); }
            catch (Exception cleanupFailure) { (failures ??= []).Add(cleanupFailure); }
            try { session?.Dispose(); }
            catch (Exception cleanupFailure) { (failures ??= []).Add(cleanupFailure); }
            try { presentation?.Dispose(); }
            catch (Exception cleanupFailure) { (failures ??= []).Add(cleanupFailure); }
            try { animation?.Dispose(); }
            catch (Exception cleanupFailure) { (failures ??= []).Add(cleanupFailure); }
            try { skyBackground?.Dispose(); }
            catch (Exception cleanupFailure) { (failures ??= []).Add(cleanupFailure); }
            if (failures is { Count: > 0 })
            {
                failures.Insert(0, constructionFailure);
                throw new AggregateException(failures);
            }
            throw;
        }
    }

    internal LoadingBayProduct(Func<ILoadingBaySession> sessionFactory)
    {
        _sessionFactory = sessionFactory ?? throw new ArgumentNullException(nameof(sessionFactory));
        _entityWorldDebug = CreateEntityWorldDebugModule();
        _session = _sessionFactory();
        _liveDebug = new LoadingBayLiveDebugModule(DebugReadout, DebugSetTrack);
        AdoptDebugWorld(_session);
    }

    /// <summary>Republishes the retained session projection for a newly attached Engine client.</summary>
    public void Attach()
    {
        if (_shutdown)
        {
            return;
        }

        RequireSession().Attach();
    }

    public void Start()
    {
        if (_shutdown)
        {
            return;
        }

        _started = true;
        _paused = false;
        RequireSession().Publish();
    }

    public ProductUpdateResult Update(ProductUpdate update)
    {
        if (!_started || _paused || _shutdown)
        {
            return ProductUpdateResult.None;
        }

        return RequireSession().Update(update);
    }

    public void Pause()
    {
        if (_started && !_shutdown)
        {
            _paused = true;
        }
    }

    public void Resume()
    {
        if (_started && !_shutdown)
        {
            _paused = false;
        }
    }

    public void Restart()
    {
        if (_shutdown)
        {
            return;
        }

        // Replacement construction is preflight-only. It must not emit a fresh HUD
        // using the prior generation's shared presentation readout.
        ILoadingBaySession replacement = _sessionFactory();
        try
        {
            if (_exitButtonAnimation is { } animation && _exitPresentation is { } presentation)
            {
                LoadingBayExitPresentationCheckpoint presentationCheckpoint = presentation.Checkpoint();
                LoadingBayExitButtonAnimationCheckpoint checkpoint = animation.ResetForGeneration();
                try
                {
                    // This is the one replacement publication that may touch shared
                    // Engine realizations. It therefore carries only fresh state.
                    replacement.ActivateSharedRealizations();
                    replacement.Publish();
                }
                catch (Exception publicationFailure)
                {
                    List<Exception>? failures = null;
                    try { replacement.DeactivateSharedRealizations(); }
                    catch (Exception rollbackFailure) { (failures ??= []).Add(rollbackFailure); }
                    try { presentation.Restore(presentationCheckpoint); }
                    catch (Exception rollbackFailure) { (failures ??= []).Add(rollbackFailure); }
                    try { animation.Restore(checkpoint); }
                    catch (Exception rollbackFailure) { (failures ??= []).Add(rollbackFailure); }
                    if (failures is { Count: > 0 })
                    {
                        failures.Insert(0, publicationFailure);
                        throw new AggregateException(failures);
                    }
                    throw;
                }
            }
            else
            {
                // The lifecycle exercise's pure session seam has no Engine-owned
                // realization to preflight, so it retains the direct publication.
                replacement.Publish();
            }
        }
        catch
        {
            replacement.Dispose();
            throw;
        }

        ILoadingBaySession previous = RequireSession();
        try
        {
            AdoptDebugWorld(replacement);
        }
        catch
        {
            replacement.Dispose();
            throw;
        }
        DetachDebugWorld(previous);
        _session = replacement;
        // Once the published replacement becomes current, it is the authoritative
        // continuation. A late old-session teardown failure cannot safely roll it
        // back, so lifecycle state commits before best-effort old-resource cleanup.
        _started = true;
        _paused = false;
        try
        {
            previous.Dispose();
        }
        catch (Exception retirementFailure)
        {
            // The replacement remains live and Restart reports success so the host
            // does not attempt a rollback against already-replaced Engine state.
            RetainRetirementFailure(retirementFailure);
        }
    }

    public void Shutdown()
    {
        if (_shutdown)
        {
            return;
        }

        _shutdown = true;
        _started = false;
        _paused = false;
        ILoadingBaySession? session = _session;
        _session = null;
        List<Exception>? failures = null;
        while (_retirementFailures.TryDequeue(out Exception? retirementFailure))
            (failures ??= []).Add(retirementFailure);
        if (_droppedRetirementFailures != 0)
            (failures ??= []).Add(new InvalidOperationException($"Loading Bay retired {_droppedRetirementFailures} additional prior-session cleanup failures."));
        try { DetachDebugWorld(session); }
        catch (Exception debugDetachFailure) { (failures ??= []).Add(debugDetachFailure); }
        try { UnregisterDebugWorld(); }
        catch (Exception debugUnregisterFailure) { (failures ??= []).Add(debugUnregisterFailure); }
        try
        {
            session?.Dispose();
        }
        catch (Exception activeCleanupFailure)
        {
            (failures ??= []).Add(activeCleanupFailure);
        }
        try
        {
            _exitPresentation?.Dispose();
        }
        catch (Exception presentationCleanupFailure)
        {
            (failures ??= []).Add(presentationCleanupFailure);
        }
        try
        {
            _exitButtonAnimation?.Dispose();
        }
        catch (Exception animationCleanupFailure)
        {
            (failures ??= []).Add(animationCleanupFailure);
        }
        try
        {
            _skyBackground?.Dispose();
        }
        catch (Exception skyCleanupFailure)
        {
            (failures ??= []).Add(skyCleanupFailure);
        }
        if (failures is { Count: > 0 }) throw new AggregateException(failures);
    }

    public void Dispose() => Shutdown();

    public void RegisterDebugCommands(IDebugCommandModuleRegistrar registrar)
    {
        ArgumentNullException.ThrowIfNull(registrar);
        registrar.Register(_liveDebug);
        registrar.Register(_entityWorldDebug);
    }

    private static ILoadingBaySession CreateSession(
        ProductCreateContext context,
        LoadingBayExitPresentation presentation,
        LoadingBayExitButtonAnimation animation,
        LoadingBaySkyBackground skyBackground)
    {
        ArgumentNullException.ThrowIfNull(context);
        ArgumentNullException.ThrowIfNull(presentation);
        ArgumentNullException.ThrowIfNull(animation);
        ArgumentNullException.ThrowIfNull(skyBackground);
        return new LoadingBaySession(context.Engine, context.Content, presentation, animation, skyBackground.Readout);
    }

    private static EntityWorldDebugModule CreateEntityWorldDebugModule()
    {
        var module = new EntityWorldDebugModule();
        module.RegisterProjection(EngineComponentTypes.Transform,
            static (in Transform value) => $"translation={Vector(value.Translation)};rotation={QuaternionValue(value.Rotation)};scale={Vector(value.Scale)}");
        module.RegisterProjection(EngineComponentTypes.SpatialCollider,
            static (in SpatialCollider value) => $"min={Vector(value.Min)};max={Vector(value.Max)};group={value.CollisionGroup};mask={value.CollisionMask};enabled={value.Enabled};static={value.StaticCollider};trigger={value.Trigger}");
        module.RegisterProjection(EngineComponentTypes.Kinematic,
            static (in Kinematic value) => $"halfExtents={Vector(value.HalfExtents)};velocity={Vector(value.Velocity)}");
        return module;
    }

    private void AdoptDebugWorld(ILoadingBaySession session)
    {
        ArgumentNullException.ThrowIfNull(session);
        if (session is ILoadingBayDebugSession debugSession)
        {
            debugSession.SetDebugEntityWorldChanged(ReplaceDebugWorld);
            if (_debugWorldRegistered)
            {
                _entityWorldDebug.ReplaceWorld(DebugWorldName, debugSession.DebugEntityWorld);
            }
            else
            {
                _entityWorldDebug.RegisterWorld(DebugWorldName, debugSession.DebugEntityWorld);
                _debugWorldRegistered = true;
            }
            return;
        }

        if (_debugWorldRegistered)
        {
            _entityWorldDebug.UnregisterWorld(DebugWorldName);
            _debugWorldRegistered = false;
        }
    }

    private void DetachDebugWorld(ILoadingBaySession? session)
    {
        if (session is ILoadingBayDebugSession debugSession)
            debugSession.SetDebugEntityWorldChanged(null);
    }

    private void UnregisterDebugWorld()
    {
        if (!_debugWorldRegistered)
            return;
        _entityWorldDebug.UnregisterWorld(DebugWorldName);
        _debugWorldRegistered = false;
    }

    private void ReplaceDebugWorld(EntityWorld world)
    {
        ArgumentNullException.ThrowIfNull(world);
        if (_debugWorldRegistered)
            _entityWorldDebug.ReplaceWorld(DebugWorldName, world);
    }

    private ILoadingBaySession RequireSession() => _session
        ?? throw new ObjectDisposedException(nameof(LoadingBayProduct));

    private string DebugReadout()
    {
        ILoadingBaySession session = RequireSession();
        LoadingBayReadout readout = session.Readout();
        LoadingBayEngineServiceReadout services = session.EngineReadout();
        LoadingBayPlayerSnapshot player = readout.PlayerState;
        return string.Join(';',
            $"lifecycle={(_shutdown ? "shutdown" : !_started ? "created" : _paused ? "paused" : "running")}",
            $"generation={readout.Generation}",
            $"step={readout.Step}",
            $"admittedSteps={readout.UpdateFacts.AdmittedStepCount}",
            $"fixedDeltaSeconds={readout.UpdateFacts.FixedDeltaSeconds.ToString("R", CultureInfo.InvariantCulture)}",
            $"position={Vector(player.Position)}",
            $"eye={Vector(player.Position + (Vector3.UnitY * readout.Tuning.EyeOffsetFromCenter))}",
            $"yawRadians={player.Look.YawRadians.ToString("R", CultureInfo.InvariantCulture)}",
            $"pitchRadians={player.Look.PitchRadians.ToString("R", CultureInfo.InvariantCulture)}",
            $"continuation={player.Continuation is not null}",
            $"health={readout.Health}",
            $"armor={readout.Armor}",
            $"ammo={readout.Bullets}",
            $"complete={readout.Complete}",
            $"facts={readout.Facts.Length}",
            $"latestFact={(readout.Facts.Length == 0 ? "none" : readout.Facts[^1].GetType().Name)}",
            $"droppedFacts={readout.DroppedFacts}",
            $"pendingSchedules={readout.PendingSchedules}",
            $"catalogPath={services.VoxelScene.CatalogPath}",
            $"catalogHash={services.VoxelScene.CatalogHash}",
            $"catalogMaterials={services.VoxelScene.MaterialCount}",
            $"catalogBoundMaterials={services.VoxelScene.BoundMaterialCount}",
            $"catalogMappings={services.VoxelScene.MappingCount}",
            $"voxelPresentation={services.VoxelScene.Realized}",
            $"voxelChunks={services.VoxelScene.SceneChunkCount}",
            $"skyPath={services.Sky.SourcePath}",
            $"skyHash={services.Sky.SourceHash}",
            $"skyBytes={services.Sky.SourceByteLength}",
            $"skyHandle={services.Sky.ResourceHandle}",
            $"skyResource={services.Sky.ResourceRealized}",
            $"skySelected={services.Sky.BackgroundSelected}");
    }

    private static string Vector(Vector3 value) => string.Create(CultureInfo.InvariantCulture, $"{value.X:R},{value.Y:R},{value.Z:R}");

    private static string QuaternionValue(Quaternion value) => string.Create(CultureInfo.InvariantCulture, $"{value.X:R},{value.Y:R},{value.Z:R},{value.W:R}");

    private DebugCommandResult DebugSetTrack(string track, int value)
    {
        if (track is not ("health" or "armor"))
            return DebugCommandResult.Failure(DebugCommandStatus.InvalidArguments, "Track must be health or armor.");
        ILoadingBaySession session = RequireSession();
        LoadingBayReceipt receipt = session.DeveloperSetTrack(session.Readout().Generation, track, value, "live-debug");
        if (!receipt.Accepted)
            return DebugCommandResult.Failure(DebugCommandStatus.Failed, receipt.Code);
        return DebugCommandResult.Success($"{track}={value};code={receipt.Code}");
    }

    private void RetainRetirementFailure(Exception failure)
    {
        if (_retirementFailures.Count == MaximumRetirementFailures)
        {
            _retirementFailures.Dequeue();
            _droppedRetirementFailures++;
        }
        _retirementFailures.Enqueue(failure);
    }
}
