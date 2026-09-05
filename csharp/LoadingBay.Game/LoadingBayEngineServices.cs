using System.Numerics;
using System.Text;
using Rusty.Engine;
using Rusty.Engine.Entities;

namespace LoadingBay.Game;

/// <summary>One call-local Engine character environment projected from canonical platform values.</summary>
internal readonly record struct LoadingBayCharacterStepEnvironment(
    CharacterSupport Support,
    ReadOnlyMemory<CharacterObstacle> Obstacles);

/// <summary>Session-owned adoption of generated Engine services, borrowing the product-owned exit animation.</summary>
internal sealed class LoadingBayEngineServices : IDisposable
{
    private readonly LoadingBayAdmittedContent _content;
    private readonly LoadingBayPlayerScene _player;
    private readonly LoadingBaySemanticPickupCoordinator _semanticPickups;
    private readonly LoadingBayWorldInteractionCoordinator _worldInteractions;
    private readonly LoadingBayEncounterCoordinator _encounters;
    private readonly LoadingBayCombatCoordinator _combat;
    private readonly LoadingBayVoxelScenePresentation _voxelPresentation;
    private readonly LoadingBayWorldServices _worldServices;
    private readonly LoadingBayHudProjection _hud;
    private readonly LoadingBaySkyReadout _skyReadout;
    private LoadingBayEngineServiceReadout _readout;
    private readonly EntityWorld _entities;
    private EntityWorld? _ownedProjectionEntities;
    private bool _disposed;

    internal LoadingBayEngineServices(
        IEngineContext engine,
        ProductContent content,
        LoadingBayTuning tuning,
        EntityWorld entities,
        EntityId playerEntity,
        LoadingBayExitPresentation exitPresentation,
        LoadingBayExitButtonAnimation exitButtonAnimation,
        LoadingBaySkyReadout skyReadout,
        bool ownsProjectionEntities = false)
    {
        _entities = entities ?? throw new ArgumentNullException(nameof(entities));
        List<IDisposable> constructed = [];
        try
        {
            _ownedProjectionEntities = ownsProjectionEntities ? _entities : null;
            _skyReadout = skyReadout;
            _content = new LoadingBayAdmittedContent(engine.Content, engine.VoxelContent, content);
            constructed.Add(_content);
            _player = new LoadingBayPlayerScene(engine, tuning);
            constructed.Add(_player);
            VoxelAssetSpatialPublishLeaseReceipt publishedScene = _player.PublishVoxelAsset(engine.VoxelContent, _content.VoxelAsset);
            _voxelPresentation = new LoadingBayVoxelScenePresentation(engine, content, _player.Session, publishedScene);
            constructed.Add(_voxelPresentation);
            _semanticPickups = new LoadingBaySemanticPickupCoordinator(engine, _player, entities, playerEntity, tuning);
            constructed.Add(_semanticPickups);
            _worldInteractions = new LoadingBayWorldInteractionCoordinator(engine, _player, entities, playerEntity, tuning);
            constructed.Add(_worldInteractions);
            _encounters = new LoadingBayEncounterCoordinator(engine.Perception, _player, playerEntity);
            _combat = new LoadingBayCombatCoordinator(engine, _player, playerEntity, tuning);
            constructed.Add(_combat);
            _worldServices = new LoadingBayWorldServices(engine, tuning, exitPresentation, exitButtonAnimation);
            constructed.Add(_worldServices);
            _player.ActivateCamera();
            _hud = new LoadingBayHudProjection(engine.Ui);
            constructed.Add(_hud);
            _readout = LoadingBayEngineServiceReadout.Empty with { Sky = _skyReadout, VoxelScene = _voxelPresentation.Readout };
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

    /// <summary>The managed entity world currently projected into this Engine service generation.</summary>
    internal EntityWorld EntityWorld => _entities;

    internal LoadingBayEngineServiceReadout Readout => _readout;

    internal void Update(
        ProductUpdate update,
        LoadingBayTuning tuning,
        Action<LoadingBayFact> record,
        Func<ulong, bool> canCollectCanonicalPickup,
        Func<ulong, LoadingBayReceipt> collectCanonicalPickup,
        Func<ulong, ulong, LoadingBayReceipt> applyCanonicalHazard,
        Func<ulong, ulong, LoadingBayReceipt> activateCanonicalFloor,
        Func<ulong, ulong, LoadingBayReceipt> activateCanonicalLift,
        Func<ulong, LoadingBayReceipt> discoverCanonicalSecret,
        Func<ulong, ulong, LoadingBayReceipt> activateCanonicalDoor,
        Func<ulong, LoadingBayReceipt> completeCanonicalExit,
        Func<LoadingBayWorldSnapshot> readWorld,
        Func<ulong, LoadingBayWeaponFirePlan?> prepareWeaponFire,
        Func<LoadingBayWeaponFirePlan, IReadOnlyList<LoadingBayWeaponImpact>, LoadingBayReceipt> settleWeaponFire,
        Func<ulong, int, ulong, LoadingBayReceipt> damageCanonicalBarrel,
        Func<IReadOnlySet<ulong>> eligibleEnemyEntities,
        Func<ulong, IReadOnlySet<ulong>, uint, uint, IReadOnlyList<LoadingBayEnemyAttackPlan>> prepareEnemyAttacks,
        Func<LoadingBayEnemyAttackPlan, bool, string, LoadingBayReceipt> settleEnemyAttack,
        Action<ulong, ulong, string> recordProjectileOutcome,
        Func<ulong, int, ulong, LoadingBayReceipt> applyProjectileDamage,
        Func<ulong, ulong, LoadingBayReceipt> activateEncounter)
    {
        ThrowIfDisposed();
        float fixedDeltaSeconds = (float)update.Facts.FixedDeltaSeconds;
        LoadingBaySemanticInput input = _player.Update(update, tuning,
            (tick, supportPresent, supportEntity) => _worldInteractions.PrepareCharacterStep(
                tick, fixedDeltaSeconds, readWorld(), supportPresent, supportEntity),
            tick =>
            {
                _semanticPickups.ReconcileMovementStep(tick, _player, canCollectCanonicalPickup, collectCanonicalPickup, record);
                _worldInteractions.ReconcileMovementStep(tick, _player, applyCanonicalHazard, activateCanonicalFloor, activateCanonicalLift, discoverCanonicalSecret, record);
                _encounters.ReconcileMovementStep(tick, activateEncounter);
                _combat.Advance(tick, fixedDeltaSeconds, prepareEnemyAttacks, settleEnemyAttack, recordProjectileOutcome, applyProjectileDamage);
            });
        _worldServices.Update(update);
        if (input.UseRequested)
        {
            record(new SemanticInputFact("player.use"));
            _worldInteractions.Use(update.Facts.SimulationStep, _player, activateCanonicalDoor, completeCanonicalExit, record);
        }
        if (input.FireRequested)
        {
            record(new SemanticInputFact("player.fire"));
            _combat.Fire(update.Facts.SimulationStep, prepareWeaponFire, settleWeaponFire, eligibleEnemyEntities, damageCanonicalBarrel);
        }
    }

    internal IReadOnlyList<CanonicalPickupTriggerStateFact> RestoreSemanticPickups(LoadingBayPickupSnapshot[] pickupStates)
    {
        ThrowIfDisposed();
        return _semanticPickups.Restore(pickupStates, _player);
    }

    internal LoadingBayPlayerSnapshot CapturePlayer() => _player.Capture();

    internal void RestorePlayer(LoadingBayPlayerSnapshot player) => _player.Restore(player, _player.Tuning);

    internal CanonicalPickupTriggerStateFact MaterializeEnemyDrop(ulong pickupEntityId, Vector3 translation, ulong tick)
    {
        ThrowIfDisposed();
        return _semanticPickups.MaterializeEnemyDrop(pickupEntityId, translation, tick);
    }

    internal bool BarrelOccluded(LoadingBayE1M1BarrelDefinition source, LoadingBayE1M1BarrelDefinition target) => _worldInteractions.BarrelOccluded(source, target);

    internal void Publish(LoadingBayReadout readout)
    {
        ThrowIfDisposed();
        LoadingBayEngineServiceReadout serviceReadout = _worldServices.Publish(readout, _player);
        serviceReadout = serviceReadout with { Sky = _skyReadout, VoxelScene = _voxelPresentation.Readout };
        _readout = serviceReadout;
        _hud.Publish(readout, LoadingBayAdmittedContent.ProjectPath, LoadingBayAdmittedContent.VoxelPath, serviceReadout);
    }

    internal void Attach(LoadingBayReadout readout)
    {
        ThrowIfDisposed();
        _voxelPresentation.Refresh();
        Publish(readout);
    }

    internal void ActivateSharedRealizations()
    {
        ThrowIfDisposed();
        _worldServices.ActivateSharedRealizations();
    }

    internal void RestoreEncounterActivations(IReadOnlySet<ulong> activeEncounterIds)
    {
        ThrowIfDisposed();
        _encounters.Restore(activeEncounterIds);
    }

    internal void RestoreWorldMotion(LoadingBayWorldSnapshot world, ulong simulationStep)
    {
        ThrowIfDisposed();
        _worldInteractions.RestoreMotion(world, simulationStep);
    }

    internal void DeactivateSharedRealizations()
    {
        ThrowIfDisposed();
        _worldServices.DeactivateSharedRealizations();
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        List<Exception>? failures = null;
        foreach (IDisposable value in new IDisposable[] { _hud, _worldServices, _combat, _worldInteractions, _semanticPickups, _voxelPresentation, _player, _content })
        {
            try { value.Dispose(); }
            catch (Exception exception) { (failures ??= []).Add(exception); }
        }
        if (_ownedProjectionEntities is not null)
        {
            try { _ownedProjectionEntities.Dispose(); }
            catch (Exception exception) { (failures ??= []).Add(exception); }
        }
        if (failures is { Count: > 0 }) throw new AggregateException(failures);
    }

    private void ThrowIfDisposed()
    {
        if (_disposed) throw new ObjectDisposedException(nameof(LoadingBayEngineServices));
    }
}

/// <summary>Validates the host-admitted E1M1 closure and retains only Engine content leases.</summary>
internal sealed class LoadingBayAdmittedContent : IDisposable
{
    internal const string ProjectPath = "projects/doom-e1m1.project.json";
    internal const string VoxelPath = "doom-e1m1/doom-e1m1.voxel.json";
    internal const string AssetCatalogPath = "doom-e1m1/doom-e1m1.asset-catalog.json";
    private static readonly ContentSha256 ProjectSha256 = new(0x08d069726cdeaf1fUL, 0xddf1181eb3e75d63UL, 0xbad11e5262a4a5b4UL, 0x6b4cf9a3bf5ae31bUL);
    private static readonly ContentSha256 VoxelSha256 = new(0x59041f8a5e291b84UL, 0x4ccc6c17ea404d58UL, 0x42bf2832633014b9UL, 0x3f223a9eeea8497bUL);

    private readonly ContentReference _project;
    private readonly ContentReference _voxel;
    private readonly VoxelAsset _voxelAsset;
    private bool _disposed;

    internal VoxelAsset VoxelAsset => _voxelAsset;

    internal LoadingBayAdmittedContent(IContentService content, IVoxelContentService voxelContent, ProductContent admitted)
    {
        ArgumentNullException.ThrowIfNull(content);
        ArgumentNullException.ThrowIfNull(voxelContent);
        ArgumentNullException.ThrowIfNull(admitted);
        RequireAdmitted(admitted, ProjectPath);
        RequireAdmitted(admitted, VoxelPath);

        ContentReference? project = null;
        ContentReference? voxel = null;
        VoxelAsset? voxelAsset = null;
        try
        {
            project = content.OpenReference(new ContentOpenRequest(ProjectPath));
            voxel = content.OpenReference(new ContentOpenRequest(VoxelPath));
            RequireExact(content.ReadReferenceInfo(project), ProjectPath, ProjectSha256);
            ContentReferenceInfo voxelInfo = RequireExact(content.ReadReferenceInfo(voxel), VoxelPath, VoxelSha256);
            if (voxelInfo.ByteLength > int.MaxValue)
                throw new InvalidOperationException($"E1M1 voxel artifact exceeds the generated byte-read limit: {voxelInfo.ByteLength}.");
            voxelAsset = voxelContent.AdmitAsset(new AdmitVoxelAssetRequest(ReadAll(content, voxel, voxelInfo.ByteLength)));
            _project = project;
            _voxel = voxel;
            _voxelAsset = voxelAsset;
        }
        catch
        {
            voxelAsset?.Dispose();
            voxel?.Dispose();
            project?.Dispose();
            throw;
        }
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        List<Exception>? failures = null;
        foreach (IDisposable value in new IDisposable[] { _voxelAsset, _voxel, _project })
        {
            try { value.Dispose(); }
            catch (Exception exception) { (failures ??= []).Add(exception); }
        }
        if (failures is { Count: > 0 }) throw new AggregateException(failures);
    }

    internal static void RequireAdmitted(ProductContent content, string path)
    {
        foreach (ProductContentFile file in content.Files.Span)
            if (file.Path.Span.SequenceEqual(Encoding.UTF8.GetBytes(path))) return;
        {
            throw new InvalidOperationException($"Host-admitted content is missing required E1M1 artifact '{path}'.");
        }
    }

    internal static ContentReferenceInfo RequireExact(ReadOnlyMemory<ContentReferenceInfo> values, string path, ContentSha256 expectedSha256)
    {
        if (values.Length != 1 || values.Span[0].Path != path || values.Span[0].Sha256 != expectedSha256 || values.Span[0].ByteLength == 0)
            throw new InvalidOperationException($"Engine Content did not retain the required E1M1 artifact '{path}' exactly once.");
        return values.Span[0];
    }

    private static ReadOnlyMemory<byte> ReadAll(IContentService content, ContentReference reference, ulong length)
    {
        const uint MaximumGeneratedRead = 1024 * 1024;
        byte[] result = new byte[checked((int)length)];
        for (ulong offset = 0; offset < length;)
        {
            uint count = checked((uint)Math.Min((ulong)MaximumGeneratedRead, length - offset));
            ReadOnlyMemory<byte> chunk = content.ReadBytes(new ContentReadBytesRequest(reference, offset, count));
            if (chunk.Length != count) throw new InvalidOperationException("Engine Content returned a truncated E1M1 voxel read.");
            chunk.CopyTo(result.AsMemory(checked((int)offset), checked((int)count)));
            offset += count;
        }
        return result;
    }
}

/// <summary>Owns one Engine spatial/character/look/camera continuation; product state remains only a pose fact.</summary>
internal sealed class LoadingBayPlayerScene : IDisposable
{
    private static readonly CameraViewport FullViewport = new(0d, 0d, 1d, 1d);
    private static readonly double RadiansToDegrees = 180d / Math.PI;
    private readonly ISpatialService _spatial;
    private readonly ICameraViewService _cameraView;
    private readonly SpatialSession _session;
    private readonly LoadingBayTuning _initialTuning;
    private Camera? _camera;
    private CharacterControllerConfig _controller;
    private Vector3 _position;
    private CharacterMotion _motion;
    private LoadingBayCharacterContinuationSnapshot? _continuation;
    private LookState _lookState;
    private Vector3 _forward;
    private Vector2 _planarIntent;
    private bool _forwardHeld;
    private bool _backwardHeld;
    private bool _leftHeld;
    private bool _rightHeld;
    private bool _jumpHeld;
    private bool _jumpPressed;
    private bool _voxelPublished;
    private ulong _sequence;
    private bool _disposed;

    internal LoadingBayPlayerScene(IEngineContext engine, LoadingBayTuning tuning)
    {
        _spatial = engine.Spatial;
        _cameraView = engine.CameraView;
        _initialTuning = tuning;
        SpatialSession? session = null;
        try
        {
            session = _spatial.CreateSession(new SpatialSessionConfig(tuning.SpatialVoxelSize, tuning.SpatialChunkSize, VoxelSurfaceMode.GreedyCubes));
            _controller = Tune(_spatial.DefaultCharacterControllerConfig(), tuning);
            _position = tuning.InitialPosition;
            LookReceipt initialLook = Look.Integrate(new LookRequest(
                new LookState(-DegreesToRadians(tuning.InitialYawDegrees), DegreesToRadians(tuning.InitialPitchDegrees)),
                Vector2.Zero,
                LookConfig(tuning)));
            _lookState = initialLook.After;
            _forward = initialLook.Forward;
            _session = session;
        }
        catch
        {
            session?.Dispose();
            throw;
        }
    }

    internal void ActivateCamera()
    {
        ThrowIfDisposed();
        if (!_voxelPublished) throw new InvalidOperationException("E1M1 voxels must be staged before camera activation.");
        if (_camera is not null) throw new InvalidOperationException("Loading Bay's E1M1 camera is already active.");
        Camera? camera = null;
        try
        {
            camera = _cameraView.CreateCamera(CameraDescriptor(_initialTuning));
            _cameraView.SetActiveCamera(camera);
            _camera = camera;
        }
        catch
        {
            camera?.Dispose();
            throw;
        }
    }

    internal SpatialSession Session => _session;

    internal Vector3 Position => _position;
    internal Vector3 Forward => _forward;
    internal LoadingBayTuning Tuning => _initialTuning;

    /// <summary>Captures only product-meaningful controller values, never the Engine session or camera handles.</summary>
    internal LoadingBayPlayerSnapshot Capture()
    {
        ThrowIfDisposed();
        return new LoadingBayPlayerSnapshot(_position, _lookState, _continuation);
    }

    /// <summary>Rebuilds the transient camera projection from a validated semantic player state.</summary>
    internal void Restore(LoadingBayPlayerSnapshot snapshot, LoadingBayTuning tuning)
    {
        ThrowIfDisposed();
        if (!Finite(snapshot.Position) || !float.IsFinite(snapshot.Look.YawRadians) || !float.IsFinite(snapshot.Look.PitchRadians))
            throw new InvalidOperationException("Snapshot supplied a non-finite E1M1 player pose or look state.");
        LookReceipt look = Look.Integrate(new LookRequest(snapshot.Look, Vector2.Zero, LookConfig(tuning)));
        if (look.After != snapshot.Look)
            throw new InvalidOperationException("Snapshot supplied an out-of-policy E1M1 player look state.");
        _position = snapshot.Position;
        _lookState = look.After;
        _forward = look.Forward;
        if (snapshot.Continuation is null)
        {
            _motion = default;
            _controller = Tune(_spatial.DefaultCharacterControllerConfig(), tuning);
            _sequence = 0;
            _continuation = null;
        }
        else
        {
            CharacterContinuationCheckpoint checkpoint = Checkpoint(snapshot.Continuation);
            CharacterContinuationRestoreReceipt restored = _spatial.RestoreCharacterContinuation(new CharacterContinuationRestoreRequest(_session, checkpoint));
            if (restored.SourceGeneration != checkpoint.SourceGeneration)
                throw new InvalidOperationException("Engine restored an incoherent E1M1 character continuation generation.");
            _motion = restored.Motion;
            _controller = checkpoint.Config;
            _sequence = restored.Motion.LastCommandSequence;
            _continuation = snapshot.Continuation;
        }
        _planarIntent = Vector2.Zero;
        _forwardHeld = _backwardHeld = _leftHeld = _rightHeld = _jumpHeld = _jumpPressed = false;
        if (_camera is not null) _cameraView.UpdateCamera(new CameraUpdateRequest(_camera, CameraDescriptor(tuning)));
    }

    internal PerceptionObserver CreatePerceptionObserver(ulong entity, LoadingBayTuning tuning)
    {
        ThrowIfDisposed();
        Vector3 forward = _forward;
        if (!float.IsFinite(forward.X) || !float.IsFinite(forward.Y) || !float.IsFinite(forward.Z) ||
            forward.LengthSquared() <= float.Epsilon)
            throw new InvalidOperationException("Loading Bay cannot query E1M1 perception with a non-finite player look direction.");
        Vector3 origin = _position + (Vector3.UnitY * tuning.EyeOffsetFromCenter);
        if (!float.IsFinite(origin.X) || !float.IsFinite(origin.Y) || !float.IsFinite(origin.Z) ||
            !double.IsFinite(tuning.PerceptionMaximumDistance) || tuning.PerceptionMaximumDistance <= 0d ||
            !double.IsFinite(tuning.PerceptionMinimumFacingCosine) || tuning.PerceptionMinimumFacingCosine is < -1d or > 1d)
            throw new InvalidOperationException("Loading Bay E1M1 perception tuning is invalid.");
        return new PerceptionObserver(entity, origin, forward, tuning.PerceptionMaximumDistance, tuning.PerceptionMinimumFacingCosine, 1d);
    }

    internal VoxelAssetSpatialPublishLeaseReceipt PublishVoxelAsset(IVoxelContentService voxelContent, VoxelAsset asset)
    {
        ThrowIfDisposed();
        ArgumentNullException.ThrowIfNull(voxelContent);
        ArgumentNullException.ThrowIfNull(asset);
        if (_voxelPublished) throw new InvalidOperationException("The fresh E1M1 spatial session already has voxel content.");
        VoxelAssetSpatialPublishLeaseReceipt receipt = voxelContent.PublishAssetToSpatial(new PublishVoxelAssetToSpatialRequest(asset, _session));
        ValidatePublishedVoxelScene(receipt);
        _voxelPublished = true;
        return receipt;
    }

    internal LoadingBaySemanticInput Update(
        ProductUpdate update,
        LoadingBayTuning tuning,
        Func<ulong, bool, ulong, LoadingBayCharacterStepEnvironment> prepareCharacterStep,
        Action<ulong> reconcileAdmittedMovementStep)
    {
        ThrowIfDisposed();
        if (_camera is null) throw new InvalidOperationException("Loading Bay's E1M1 camera is not active.");
        if (!_voxelPublished) throw new InvalidOperationException("Loading Bay's E1M1 voxel scene is not staged.");
        LoadingBaySemanticInput input = ApplyInput(update.Input, tuning);
        if (update.Facts.AdmittedStepCount > 0 && double.IsFinite(update.Facts.FixedDeltaSeconds) && update.Facts.FixedDeltaSeconds > 0d && update.Facts.FixedDeltaSeconds <= float.MaxValue)
        {
            float delta = (float)update.Facts.FixedDeltaSeconds;
            for (uint step = 0; step < update.Facts.AdmittedStepCount; step++)
            {
                ulong tick = LoadingBayAdmittedStepTicks.At(update.Facts, step);
                // Platform motion is admitted before the character proposal. The public Engine
                // continuation then compares this current platform transform with the prior
                // support transform retained in CharacterMotion and carries the player itself.
                LoadingBayCharacterStepEnvironment environment = prepareCharacterStep(tick, _motion.SupportEntityPresent, _motion.SupportEntity);
                CharacterStepReceipt receipt = _spatial.ProposeCharacterStep(new CharacterStepRequest(
                    _session, _position, _motion, environment.Support, environment.Obstacles, _controller,
                    new CharacterControllerCommand(_planarIntent, _lookState.YawRadians, _jumpPressed, _jumpHeld, false, Vector3.Zero, Vector3.Zero, delta, ++_sequence)));
                _position = receipt.Transform.Translation;
                _motion = receipt.Motion;
                _continuation = Copy(_spatial.CaptureCharacterContinuation(new CharacterContinuationCaptureRequest(_session, receipt.Generation)));
                _jumpPressed = false;
                reconcileAdmittedMovementStep(tick);
            }
            _cameraView.UpdateCamera(new CameraUpdateRequest(_camera, CameraDescriptor(tuning)));
        }
        return input;
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        List<Exception>? failures = null;
        if (_camera is not null)
        {
            try { _camera.Dispose(); }
            catch (Exception exception) { (failures ??= []).Add(exception); }
        }
        try { _session.Dispose(); }
        catch (Exception exception) { (failures ??= []).Add(exception); }
        if (failures is { Count: > 0 }) throw new AggregateException(failures);
    }

    private LoadingBaySemanticInput ApplyInput(ReadOnlySpan<ProductInputEvent> inputs, LoadingBayTuning tuning)
    {
        bool use = false;
        bool fire = false;
        foreach (ProductInputEvent input in inputs)
        {
            if (input.Kind == InputEventKind.Clear)
            {
                _forwardHeld = _backwardHeld = _leftHeld = _rightHeld = false;
                _jumpHeld = _jumpPressed = false;
                _planarIntent = Vector2.Zero;
                continue;
            }
            if (input.Kind == InputEventKind.PointerDelta)
            {
                LookReceipt integrated = Look.Integrate(new LookRequest(_lookState, new Vector2(input.X, input.Y), LookConfig(tuning)));
                _lookState = integrated.After;
                _forward = integrated.Forward;
                continue;
            }
            if (input.Kind is not (InputEventKind.DirectDigital or InputEventKind.MappedDigital or InputEventKind.DirectAxis or InputEventKind.MappedAxis)) continue;
            bool active = input.Edge != InputEdge.Released && input.X > 0f;
            if (input.Intent.Span.SequenceEqual("player.move.forward"u8)) _forwardHeld = active;
            else if (input.Intent.Span.SequenceEqual("player.move.backward"u8)) _backwardHeld = active;
            else if (input.Intent.Span.SequenceEqual("player.move.left"u8)) _leftHeld = active;
            else if (input.Intent.Span.SequenceEqual("player.move.right"u8)) _rightHeld = active;
            else if (input.Intent.Span.SequenceEqual("player.jump"u8))
            {
                _jumpPressed |= input.Edge == InputEdge.Pressed;
                _jumpHeld = active;
            }
            else if (active && input.Intent.Span.SequenceEqual("player.use"u8)) use = true;
            else if (active && input.Intent.Span.SequenceEqual("player.fire"u8)) fire = true;
        }
        _planarIntent = new Vector2((_rightHeld ? 1f : 0f) - (_leftHeld ? 1f : 0f), (_forwardHeld ? 1f : 0f) - (_backwardHeld ? 1f : 0f));
        return new LoadingBaySemanticInput(use, fire);
    }

    private CameraDescriptor CameraDescriptor(LoadingBayTuning tuning) => new(
        new CameraPose(_position + (Vector3.UnitY * tuning.EyeOffsetFromCenter), _lookState.PitchRadians * RadiansToDegrees, _lookState.YawRadians * RadiansToDegrees),
        CameraBasisMode.Derived, default,
        new CameraProjection(CameraProjectionKind.Perspective, tuning.CameraFieldOfViewDegrees, 0d, tuning.CameraNearPlane, tuning.CameraFarPlane), FullViewport);

    private static CharacterControllerConfig Tune(CharacterControllerConfig configuration, LoadingBayTuning tuning) => configuration with
    {
        Shape = configuration.Shape with
        {
            StandingHeight = tuning.StandingCharacterHeight,
            CrouchedHeight = tuning.CrouchedCharacterHeight,
            Radius = tuning.CharacterRadius,
        },
        Ground = configuration.Ground with { ForwardSpeed = tuning.MovementSpeed, BackwardSpeed = tuning.MovementSpeed, StrafeSpeed = tuning.MovementSpeed },
        Vertical = configuration.Vertical with { Gravity = tuning.Gravity, JumpSpeed = tuning.JumpSpeed },
        Surface = configuration.Surface with { MaximumStepHeight = tuning.MaximumStepHeight },
    };

    private static LookConfig LookConfig(LoadingBayTuning tuning)
    {
        float radiansPerUnit = DegreesToRadians(tuning.LookDegreesPerUnit);
        return new LookConfig(radiansPerUnit, radiansPerUnit, -1.5f, 1.5f, 1f, false, false, true);
    }

    private static float DegreesToRadians(float degrees) => degrees * (MathF.PI / 180f);
    private static bool Finite(Vector3 value) => float.IsFinite(value.X) && float.IsFinite(value.Y) && float.IsFinite(value.Z);
    private static LoadingBayCharacterContinuationSnapshot Copy(CharacterContinuationCheckpoint checkpoint) => new(checkpoint.SourceSessionIdentity, checkpoint.SourceGeneration, checkpoint.SpatialSessionFingerprint, checkpoint.ContentAuthorityHash, checkpoint.ConfigFingerprint, checkpoint.Config, checkpoint.Motion);
    private static CharacterContinuationCheckpoint Checkpoint(LoadingBayCharacterContinuationSnapshot checkpoint) => new(checkpoint.SourceSessionIdentity, checkpoint.SourceGeneration, checkpoint.SpatialSessionFingerprint, checkpoint.ContentAuthorityHash, checkpoint.ConfigFingerprint, checkpoint.Config, checkpoint.Motion);

    private static void ValidatePublishedVoxelScene(VoxelAssetSpatialPublishLeaseReceipt receipt)
    {
        if (receipt.Palette.Length == 0 || receipt.SolidVoxelCount == 0 || receipt.ResidentChunkCount == 0 ||
            !double.IsFinite(receipt.VoxelSize) || receipt.VoxelSize <= 0d || receipt.ChunkSize == 0 || receipt.AuthorityHash == 0 ||
            receipt.VoxelDataHash == default || receipt.ContentHash == default)
            throw new InvalidOperationException("Engine did not publish a complete E1M1 voxel scene into the fresh spatial session.");

        HashSet<uint> slots = [];
        HashSet<string> materials = new(StringComparer.Ordinal);
        foreach (VoxelAssetSpatialPaletteRow row in receipt.Palette.Span)
            if (row.MaterialSlot == 0 || string.IsNullOrWhiteSpace(row.MaterialAssetId) || string.IsNullOrWhiteSpace(row.DisplayName) ||
                !slots.Add(row.MaterialSlot) || !materials.Add(row.MaterialAssetId))
                throw new InvalidOperationException("Engine voxel publication returned duplicate or incomplete E1M1 palette state.");
    }

    private void ThrowIfDisposed()
    {
        if (_disposed) throw new ObjectDisposedException(nameof(LoadingBayPlayerScene));
    }
}

internal readonly record struct LoadingBaySemanticInput(bool UseRequested, bool FireRequested);

/// <summary>Derives every per-step Spatial tick from the one host-admitted update fact.</summary>
internal static class LoadingBayAdmittedStepTicks
{
    internal static ulong At(ProductUpdateFacts facts, uint admittedIndex)
    {
        if (admittedIndex >= facts.AdmittedStepCount)
            throw new InvalidOperationException("Host-admitted movement steps do not expose a coherent simulation tick range.");
        return checked(facts.SimulationStep + admittedIndex);
    }
}

/// <summary>Engine Perception evaluates authored encounter-radius inclusion; product policy only admits the resulting edge once.</summary>
internal sealed class LoadingBayEncounterCoordinator
{
    private readonly IPerceptionService _perception;
    private readonly LoadingBayPlayerScene _player;
    private readonly EntityId _playerEntity;
    private readonly HashSet<ulong> _active = [];

    internal LoadingBayEncounterCoordinator(IPerceptionService perception, LoadingBayPlayerScene player, EntityId playerEntity)
    {
        _perception = perception ?? throw new ArgumentNullException(nameof(perception));
        _player = player ?? throw new ArgumentNullException(nameof(player));
        _playerEntity = playerEntity;
    }

    internal void ReconcileMovementStep(ulong tick, Func<ulong, ulong, LoadingBayReceipt> activate)
    {
        foreach (LoadingBayE1M1EncounterDefinition encounter in LoadingBayE1M1SemanticCatalog.Encounters)
        {
            if (_active.Contains(encounter.EntityId)) continue;
            PerceptionReadoutLeaseReceipt readout = _perception.QueryVisibility(new PerceptionQueryRequest(
                _player.Session,
                new[] { new PerceptionObserver(encounter.EntityId, encounter.Translation, Vector3.UnitZ, encounter.ActivationRadius, -1d, 1d) },
                new[] { new PerceptionTarget(_playerEntity.Value, _player.Position) },
                ReadOnlyMemory<SpatialEntityCollider>.Empty,
                0,
                0,
                64));
            bool inRange = false;
            foreach (PerceptionPair pair in readout.Pairs.Span)
                if (pair.Observer == encounter.EntityId && pair.Target == _playerEntity.Value) { inRange = true; break; }
            if (!inRange) continue;
            LoadingBayReceipt outcome = activate(encounter.EntityId, tick);
            if (!outcome.Accepted) throw new InvalidOperationException($"Canonical encounter {encounter.EntityId} overlap could not settle: {outcome.Code}.");
            _active.Add(encounter.EntityId);
        }
    }

    internal void Restore(IReadOnlySet<ulong> activeEncounterIds)
    {
        if (activeEncounterIds.Any(id => !LoadingBayE1M1SemanticCatalog.Encounters.Any(encounter => encounter.EntityId == id)))
            throw new InvalidOperationException("Snapshot supplied an unknown E1M1 encounter activation.");
        _active.Clear();
        _active.UnionWith(activeEncounterIds);
    }
}

/// <summary>Uses public Engine keyed RNG and raycasts; it retains no combat state or geometry authority.</summary>
internal sealed class LoadingBayCombatCoordinator : IDisposable
{
    private const string RandomScope = "loading-bay.e1m1.combat";
    private readonly IRandomService _random;
    private readonly ISpatialService _spatial;
    private readonly IPerceptionService _perception;
    private readonly IDynamicsService _dynamics;
    private readonly LoadingBayPlayerScene _player;
    private readonly LoadingBayTuning _tuning;
    private readonly EntityId _playerEntity;
    private readonly DynamicsWorld _projectileWorld;
    private readonly SpatialEntityCollider[] _enemyHitboxes;
    private readonly SpatialEntityCollider[] _barrelHitboxes;
    private readonly Vector3 _playerHalfExtents;
    private readonly List<ActiveProjectile> _projectiles = [];
    private bool _disposed;

    internal LoadingBayCombatCoordinator(IEngineContext engine, LoadingBayPlayerScene player, EntityId playerEntity, LoadingBayTuning tuning)
    {
        _random = engine.Random;
        _spatial = engine.Spatial;
        _perception = engine.Perception;
        _dynamics = engine.Dynamics;
        _player = player;
        _tuning = tuning;
        _playerEntity = playerEntity;
        _playerHalfExtents = tuning.PlayerPickupHalfExtents;
        _enemyHitboxes = LoadingBayE1M1SemanticCatalog.Enemies.Select(enemy => new SpatialEntityCollider(enemy.EntityId, enemy.Translation - enemy.HitboxHalfExtents, enemy.Translation + enemy.HitboxHalfExtents, 2, uint.MaxValue, true, false, false)).ToArray();
        _barrelHitboxes = LoadingBayE1M1SemanticCatalog.Barrels.Select(barrel => new SpatialEntityCollider(barrel.EntityId, barrel.Translation - barrel.HitboxHalfExtents, barrel.Translation + barrel.HitboxHalfExtents, 2, uint.MaxValue, true, false, false)).ToArray();
        _projectileWorld = _dynamics.CreateWorld(new DynamicsWorldConfig(new Vector3(0f, -tuning.Gravity, 0f)));
        try { _dynamics.BindWorldCollision(new DynamicsWorldCollisionBindingRequest(_projectileWorld, _player.Session)); }
        catch { _projectileWorld.Dispose(); throw; }
    }

    internal void Fire(ulong tick, Func<ulong, LoadingBayWeaponFirePlan?> prepare, Func<LoadingBayWeaponFirePlan, IReadOnlyList<LoadingBayWeaponImpact>, LoadingBayReceipt> settle, Func<IReadOnlySet<ulong>> eligibleEnemyEntities, Func<ulong, int, ulong, LoadingBayReceipt> damageBarrel)
    {
        LoadingBayWeaponFirePlan? plan = prepare(tick);
        if (plan is null) return;
        List<LoadingBayWeaponImpact> impacts = [];
        IReadOnlySet<ulong> eligibleEnemies = eligibleEnemyEntities();
        SpatialEntityCollider[] eligibleHitboxes = _enemyHitboxes.Where(hitbox => eligibleEnemies.Contains(hitbox.Entity)).Concat(_barrelHitboxes).ToArray();
        for (int pellet = 0; pellet < plan.PelletCount; pellet++)
        {
            int damage = 0;
            for (int roll = 0; roll < plan.DamageRolls; roll++) damage += checked((int)_random.DrawKeyed(new KeyedRngRequest(tick, RandomScope, $"{plan.WeaponId}.pellet.{pellet}.roll.{roll}", 1, plan.Damage)).Value);
            Vector3 direction = SpreadDirection(plan, pellet, tick);
            SpatialHit hit = _spatial.CastRay(new SpatialRaycastRequest(_player.Session, _player.Position, direction, plan.MaximumDistance, new SpatialQueryFilter(1, uint.MaxValue), eligibleHitboxes, new ulong[] { _playerEntity.Value }, eligibleHitboxes));
            bool barrel = hit.Present && hit.Kind == SpatialHitKind.Entity && _barrelHitboxes.Any(hitbox => hitbox.Entity == hit.Entity);
            if (barrel)
            {
                LoadingBayReceipt barrelOutcome = damageBarrel(hit.Entity, damage, tick);
                if (!barrelOutcome.Accepted) throw new InvalidOperationException($"Canonical barrel hit could not settle: {barrelOutcome.Code}.");
            }
            bool enemy = hit.Present && hit.Kind == SpatialHitKind.Entity && eligibleEnemies.Contains(hit.Entity);
            impacts.Add(new LoadingBayWeaponImpact(enemy ? hit.Entity : 0, damage, pellet, hit.Present && !enemy));
        }
        _ = settle(plan, impacts);
    }

    /// <summary>Runs once per host-admitted movement step: Engine visibility first, then product readiness, then Engine attack realization.</summary>
    internal void Advance(
        ulong tick,
        float fixedDeltaSeconds,
        Func<ulong, IReadOnlySet<ulong>, uint, uint, IReadOnlyList<LoadingBayEnemyAttackPlan>> prepare,
        Func<LoadingBayEnemyAttackPlan, bool, string, LoadingBayReceipt> settle,
        Action<ulong, ulong, string> recordProjectileOutcome,
        Func<ulong, int, ulong, LoadingBayReceipt> applyProjectileDamage)
    {
        ThrowIfDisposed();
        if (!float.IsFinite(fixedDeltaSeconds) || fixedDeltaSeconds <= 0f) throw new InvalidOperationException("Engine admitted an invalid E1M1 combat step duration.");
        PerceptionReadoutLeaseReceipt perception = ObserveEnemies();
        HashSet<ulong> visible = [];
        foreach (PerceptionPair pair in perception.Pairs.Span)
            if (pair.Target == _playerEntity.Value && pair.Kind == PerceptionPairKind.Visible) visible.Add(pair.Observer);
        foreach (LoadingBayEnemyAttackPlan plan in prepare(tick, visible, perception.VisibilityCasts, perception.OcclusionRejects))
        {
            if (plan.Kind == LoadingBayE1M1EnemyAttackKind.Hitscan)
            {
                bool hitPlayer = HitscanReachesPlayer(plan);
                _ = settle(plan, hitPlayer, hitPlayer ? "combat.hitscan-player" : "combat.hitscan-occluded");
            }
            else
            {
                RealizeProjectile(plan);
                _ = settle(plan, false, "combat.projectile-launched");
            }
        }
        StepProjectiles(tick, fixedDeltaSeconds, recordProjectileOutcome, applyProjectileDamage);
    }

    private PerceptionReadoutLeaseReceipt ObserveEnemies()
    {
        LoadingBayE1M1EnemyDefinition[] actors = LoadingBayE1M1SemanticCatalog.Enemies;
        PerceptionObserver[] observers = actors.Select(enemy =>
        {
            Vector3 origin = enemy.Translation + enemy.AttackOriginOffset;
            Vector3 towardPlayer = _player.Position - origin;
            Vector3 forward = towardPlayer.LengthSquared() > float.Epsilon ? Vector3.Normalize(towardPlayer) : Vector3.UnitZ;
            return new PerceptionObserver(enemy.EntityId, origin, forward, enemy.SightRange, -1d, 1d);
        }).ToArray();
        PerceptionReadoutLeaseReceipt receipt = _perception.QueryVisibility(new PerceptionQueryRequest(
            _player.Session,
            observers,
            new[] { new PerceptionTarget(_playerEntity.Value, _player.Position) },
            ReadOnlyMemory<SpatialEntityCollider>.Empty,
            0,
            0,
            64));
        if (receipt.SelectedObservers != (uint)observers.Length || receipt.SelectedTargets != 1 || receipt.Pairs.Length > observers.Length)
            throw new InvalidOperationException("Engine Perception did not retain the bounded E1M1 enemy observation set.");
        HashSet<ulong> returnedObservers = [];
        foreach (PerceptionPair pair in receipt.Pairs.Span)
            if (!returnedObservers.Add(pair.Observer) || !LoadingBayE1M1SemanticCatalog.Enemies.Any(enemy => enemy.EntityId == pair.Observer) || pair.Target != _playerEntity.Value || !double.IsFinite(pair.Distance) || !double.IsFinite(pair.FacingCosine))
                throw new InvalidOperationException("Engine Perception returned an invalid E1M1 enemy observation pair.");
        // Engine omits distance-rejected observers; no pair is therefore a typed not-visible outcome.
        return receipt;
    }

    private bool HitscanReachesPlayer(LoadingBayEnemyAttackPlan plan)
    {
        Vector3 target = _player.Position + (Vector3.UnitY * _playerHalfExtents.Y);
        Vector3 delta = target - plan.Origin;
        double distance = delta.Length();
        if (!double.IsFinite(distance) || distance <= 0d || distance > plan.Range) return false;
        SpatialEntityCollider playerHitbox = new(_playerEntity.Value, _player.Position - _playerHalfExtents, _player.Position + _playerHalfExtents, 1, uint.MaxValue, true, false, false);
        SpatialHit hit = _spatial.CastSegment(new SpatialSegmentCastRequest(
            _player.Session, plan.Origin, target, new SpatialQueryFilter(2, uint.MaxValue), new[] { playerHitbox }, new[] { plan.EnemyEntityId }, new[] { playerHitbox }));
        return hit.Present && hit.Kind == SpatialHitKind.Entity && hit.Entity == _playerEntity.Value;
    }

    private void RealizeProjectile(LoadingBayEnemyAttackPlan plan)
    {
        if (plan.ProjectileMass <= 0f || plan.ProjectileRadius <= 0f || plan.ProjectileImpulse <= 0f || plan.ProjectileLifetimeTicks <= 0)
            throw new InvalidOperationException($"Canonical E1M1 projectile plan for enemy {plan.EnemyEntityId} is invalid.");
        Vector3 target = _player.Position + (Vector3.UnitY * _playerHalfExtents.Y);
        Vector3 direction = Vector3.Normalize(target - plan.Origin);
        DynamicsMassPolicy massPolicy = new(DynamicsMassPolicyKind.DeriveFromShapeAndMass, default);
        DynamicsBodyProperties properties = new(plan.ProjectileMass, massPolicy, Vector3.Zero, Vector3.Zero,
            new AxisLocks(false, false, false, false, false, false), _tuning.ProjectileLinearDamping, _tuning.ProjectileAngularDamping,
            plan.ProjectileGravityScale, _tuning.ProjectileFriction, plan.ProjectileRestitution, uint.MaxValue, uint.MaxValue, true, false, false);
        DynamicsBody body = _dynamics.CreateSphereBodyWithProperties(new DynamicsCreateSphereBodyPropertiesRequest(_projectileWorld,
            new DynamicsSphereBodyPropertiesConfig(new Transform(plan.Origin, Quaternion.Identity, Vector3.One), plan.ProjectileRadius, properties)));
        _projectiles.Add(new ActiveProjectile(plan.EnemyEntityId, body, checked(plan.Tick + (ulong)plan.ProjectileLifetimeTicks), direction * plan.ProjectileImpulse, plan.Damage, plan.Origin));
    }

    private void StepProjectiles(ulong tick, float fixedDeltaSeconds, Action<ulong, ulong, string> recordOutcome, Func<ulong, int, ulong, LoadingBayReceipt> applyDamage)
    {
        if (_projectiles.Count == 0) return;
        DynamicsAction[] actions = _projectiles.Select(projectile => new DynamicsAction(projectile.Body, Vector3.Zero, Vector3.Zero, projectile.PendingImpulse, Vector3.Zero, true)).ToArray();
        DynamicsStepAndReadLeaseReceipt receipt = _dynamics.StepAndRead(new DynamicsStepAndReadRequest(_projectileWorld, fixedDeltaSeconds, 1, actions, _projectiles.Select(projectile => projectile.Body).ToArray()));
        if (receipt.BodyCount != _projectiles.Count) throw new InvalidOperationException("Engine Dynamics did not retain all active E1M1 projectile bodies.");
        foreach (ActiveProjectile projectile in _projectiles) projectile.PendingImpulse = Vector3.Zero;
        for (int index = _projectiles.Count - 1; index >= 0; index--)
        {
            ActiveProjectile projectile = _projectiles[index];
            DynamicsStepAndReadBody? readout = null;
            foreach (DynamicsStepAndReadBody candidate in receipt.Bodies.Span)
                if (candidate.Body.Value == projectile.Body.Handle.Value) { readout = candidate; break; }
            if (readout is null) throw new InvalidOperationException("Engine Dynamics omitted an active E1M1 projectile body.");
            bool hitPlayer = ProjectileReachesPlayer(projectile.LastTranslation, readout.Value.Readout.Transform.Translation, projectile.EnemyEntityId);
            bool collidedWithWorld = readout.Value.Readout.FirstContact.Present && readout.Value.Readout.FirstContact.Environment;
            if (!hitPlayer && !collidedWithWorld && tick < projectile.ExpiresAtTick)
            {
                projectile.LastTranslation = readout.Value.Readout.Transform.Translation;
                continue;
            }
            if (hitPlayer) _ = applyDamage(projectile.EnemyEntityId, projectile.Damage, tick);
            recordOutcome(projectile.EnemyEntityId, tick, hitPlayer ? "combat.projectile-player-impact" : collidedWithWorld ? "combat.projectile-world-impact" : "combat.projectile-expired");
            projectile.Body.Dispose();
            _projectiles.RemoveAt(index);
        }
    }
    private Vector3 SpreadDirection(LoadingBayWeaponFirePlan plan, int pellet, ulong tick)
    {
        if (plan.SpreadDegrees == 0d) return _player.Forward;
        double horizontal = _random.DrawKeyed(new KeyedRngRequest(tick, RandomScope, $"{plan.WeaponId}.pellet.{pellet}.spread.horizontal", -10_000, 10_000)).Value / 10_000d;
        double vertical = _random.DrawKeyed(new KeyedRngRequest(tick, RandomScope, $"{plan.WeaponId}.pellet.{pellet}.spread.vertical", -10_000, 10_000)).Value / 10_000d;
        float radians = (float)(plan.SpreadDegrees * Math.PI / 180d);
        Vector3 forward = _player.Forward;
        Vector3 right = Vector3.Normalize(Vector3.Cross(forward, Vector3.UnitY));
        if (!float.IsFinite(right.X) || right.LengthSquared() <= float.Epsilon) right = Vector3.UnitX;
        return Vector3.Normalize(forward + (right * (float)(horizontal * radians)) + (Vector3.UnitY * (float)(vertical * radians)));
    }

    private bool ProjectileReachesPlayer(Vector3 start, Vector3 end, ulong enemyEntityId)
    {
        SpatialEntityCollider playerHitbox = new(_playerEntity.Value, _player.Position - _playerHalfExtents, _player.Position + _playerHalfExtents, 1, uint.MaxValue, true, false, false);
        SpatialHit hit = _spatial.CastSegment(new SpatialSegmentCastRequest(
            _player.Session, start, end, new SpatialQueryFilter(2, uint.MaxValue), new[] { playerHitbox }, new[] { enemyEntityId }, new[] { playerHitbox }));
        return hit.Present && hit.Kind == SpatialHitKind.Entity && hit.Entity == _playerEntity.Value;
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        List<Exception>? failures = null;
        foreach (ActiveProjectile projectile in _projectiles)
        {
            try { projectile.Body.Dispose(); }
            catch (Exception exception) { (failures ??= []).Add(exception); }
        }
        _projectiles.Clear();
        try { _projectileWorld.Dispose(); }
        catch (Exception exception) { (failures ??= []).Add(exception); }
        if (failures is { Count: > 0 }) throw new AggregateException(failures);
    }

    private void ThrowIfDisposed()
    {
        if (_disposed) throw new ObjectDisposedException(nameof(LoadingBayCombatCoordinator));
    }

    private sealed class ActiveProjectile(ulong enemyEntityId, DynamicsBody body, ulong expiresAtTick, Vector3 pendingImpulse, int damage, Vector3 lastTranslation)
    {
        internal ulong EnemyEntityId { get; } = enemyEntityId;
        internal DynamicsBody Body { get; } = body;
        internal ulong ExpiresAtTick { get; } = expiresAtTick;
        internal Vector3 PendingImpulse { get; set; } = pendingImpulse;
        internal int Damage { get; } = damage;
        internal Vector3 LastTranslation { get; set; } = lastTranslation;
    }
}

/// <summary>
/// Engine owns E1M1 trigger lifecycle and overlap truth; Loading Bay owns typed item policy
/// and resulting facts. Canonical IDs are bootstrapped before ephemeral inventory entities.
/// </summary>
internal sealed class LoadingBaySemanticPickupCoordinator : IDisposable
{
    private const string TriggerScope = "loading-bay.e1m1.pickup";
    // Engine trigger tags are mechanism identifiers; typed program IDs remain product data/facts.
    private const string TriggerTag = "pickup";
    private readonly EntityWorld _entities;
    private readonly SpatialEntityWorld _spatialEntities;
    private readonly ISpatialService _spatial;
    private readonly SpatialSession _session;
    private readonly EntityId _player;
    private readonly LoadingBayE1M1PickupPlacement[] _pickups;
    private readonly Dictionary<ulong, Vector3> _pickupTranslations = [];
    private readonly Vector3 _playerHalfExtents;
    private readonly int _maximumEntities;
    private readonly int _maximumFactReadback;
    private bool _disposed;

    internal LoadingBaySemanticPickupCoordinator(IEngineContext engine, LoadingBayPlayerScene player, EntityWorld entities, EntityId playerEntity, LoadingBayTuning tuning)
    {
        ArgumentNullException.ThrowIfNull(engine);
        ArgumentNullException.ThrowIfNull(player);
        _entities = entities ?? throw new ArgumentNullException(nameof(entities));
        _spatial = engine.Spatial;
        _session = player.Session;
        _pickups = LoadingBayE1M1SemanticCatalog.Pickups;
        if (_pickups.Length != tuning.MaximumPickupBindings)
            throw new InvalidOperationException("E1M1 pickup binding count drifted from the named product bound.");
        _playerHalfExtents = tuning.PlayerPickupHalfExtents;
        _maximumEntities = checked((int)tuning.MaximumSpatialEntityBindings);
        if (_maximumEntities < _pickups.Length + 1)
            throw new InvalidOperationException("E1M1 spatial reconciliation bound cannot exclude canonical pickup or player rows.");
        _maximumFactReadback = checked((int)tuning.MaximumPickupFactReadback);
        _player = playerEntity;
        if (_player.Value != 1 || !_entities.IsAlive(_player) || _entities.NextEntityValue <= LoadingBayE1M1SemanticCatalog.CanonicalEntityCount)
            throw new InvalidOperationException("The E1M1 canonical world was not bootstrapped before ephemeral entities were allocated.");

        foreach (LoadingBayE1M1PickupPlacement pickup in _pickups)
        {
            _pickupTranslations.Add(pickup.EntityId, pickup.Translation);
            _entities.Set(new EntityId(pickup.EntityId), EngineComponentTypes.Transform,
                new Transform(pickup.Translation, Quaternion.Identity, Vector3.One));
            _entities.Set(new EntityId(pickup.EntityId), EngineComponentTypes.SpatialCollider,
                new SpatialCollider(pickup.BoundsMin, pickup.BoundsMax, 0, 0, false, true, true));
        }
        _entities.Set(_player, EngineComponentTypes.Transform,
            new Transform(player.Position, Quaternion.Identity, Vector3.One));
        _entities.Set(_player, EngineComponentTypes.SpatialCollider,
            new SpatialCollider(-tuning.PlayerPickupHalfExtents, tuning.PlayerPickupHalfExtents, 1, uint.MaxValue, true, false, false));
        _spatialEntities = new SpatialEntityWorld(_entities, _spatial, _session, EngineComponentTypes.SpatialCollider);
        foreach (LoadingBayE1M1PickupPlacement pickup in _pickups)
        {
            _spatial.RegisterTrigger(new SpatialTriggerRegisterRequest(_session, pickup.EntityId, TriggerScope, TriggerTag, SpatialTriggerGeometry.EntityBounds));
            if (pickup.StartsDormant)
            {
                SpatialTriggerReadReceipt state = _spatial.ReadTrigger(new SpatialTriggerReadRequest(_session, pickup.EntityId));
                _spatial.SetTriggerActive(new SpatialTriggerSetActiveRequest(_session, pickup.EntityId, state.Revision, false, 0));
            }
        }
    }

    internal void ReconcileMovementStep(
        ulong tick,
        LoadingBayPlayerScene player,
        Func<ulong, bool> canCollect,
        Func<ulong, LoadingBayReceipt> collect,
        Action<LoadingBayFact> record)
    {
        ThrowIfDisposed();
        _entities.Set(_player, EngineComponentTypes.Transform, new Transform(player.Position, Quaternion.Identity, Vector3.One));
        SpatialEntityWorldReconcileReceipt receipt = _spatialEntities.ReconcileTriggers(
            tick, SpatialTriggerCause.Movement, maximumEntities: _maximumEntities, maximumFactReadback: _maximumFactReadback);
        if (receipt.FactsTruncated) throw new InvalidOperationException("E1M1 pickup trigger facts exceeded the deliberate readback bound.");
        foreach (SpatialTriggerFactAtReceipt fact in receipt.Facts.Span)
        {
            if (!fact.Present || !fact.Enter || fact.Subject != _player.Value || !_pickups.Any(pickup => pickup.EntityId == fact.Trigger)) continue;
            if (!canCollect(fact.Trigger))
            {
                LoadingBayReceipt rejected = collect(fact.Trigger);
                record(new CanonicalPickupOverlapFact(fact.Trigger, fact.Subject, fact.Tick, rejected.Accepted, rejected.Code));
                SpatialTriggerReadReceipt observed = _spatial.ReadTrigger(new SpatialTriggerReadRequest(_session, fact.Trigger));
                LoadingBayE1M1PickupPlacement pickup = LoadingBayE1M1SemanticCatalog.Pickup(fact.Trigger);
                record(new PickupLifecycleFact(fact.Trigger, pickup.ItemId, pickup.ProgramId, LoadingBayPickupLifecycle.Active, rejected.Code, fact.Tick, observed.Revision));
                continue;
            }

            SpatialTriggerReadReceipt before = _spatial.ReadTrigger(new SpatialTriggerReadRequest(_session, fact.Trigger));
            SpatialTriggerLifecycleReceipt retired = _spatial.SetTriggerActive(
                new SpatialTriggerSetActiveRequest(_session, fact.Trigger, before.Revision, false, fact.Tick));
            LoadingBayReceipt outcome;
            try
            {
                outcome = collect(fact.Trigger);
            }
            catch (Exception collectionFailure)
            {
                Reactivate(fact.Trigger, retired.RevisionAfter, fact.Tick, collectionFailure);
                throw;
            }
            if (!outcome.Accepted)
            {
                SpatialTriggerLifecycleReceipt reactivated = Reactivate(fact.Trigger, retired.RevisionAfter, fact.Tick,
                    new InvalidOperationException($"Preflighted E1M1 pickup settlement rejected with {outcome.Code}."));
                record(new CanonicalPickupOverlapFact(fact.Trigger, fact.Subject, fact.Tick, false, outcome.Code));
                LoadingBayE1M1PickupPlacement rejected = LoadingBayE1M1SemanticCatalog.Pickup(fact.Trigger);
                record(new PickupLifecycleFact(fact.Trigger, rejected.ItemId, rejected.ProgramId, LoadingBayPickupLifecycle.Active, outcome.Code, fact.Tick, reactivated.RevisionAfter));
                continue;
            }
            record(new CanonicalPickupOverlapFact(fact.Trigger, fact.Subject, fact.Tick, outcome.Accepted, outcome.Code));
            record(new CanonicalPickupTriggerStateFact(
                fact.Trigger, retired.Active, retired.RevisionBefore, retired.RevisionAfter,
                retired.RemovedOverlapCount, "pickup.collected"));
            LoadingBayE1M1PickupPlacement collected = LoadingBayE1M1SemanticCatalog.Pickup(fact.Trigger);
            record(new PickupLifecycleFact(fact.Trigger, collected.ItemId, collected.ProgramId, LoadingBayPickupLifecycle.Collected, outcome.Code, fact.Tick, retired.RevisionAfter));
        }
    }

    internal IReadOnlyList<CanonicalPickupTriggerStateFact> Restore(LoadingBayPickupSnapshot[] pickupStates, LoadingBayPlayerScene player)
    {
        ThrowIfDisposed();
        _entities.Set(_player, EngineComponentTypes.Transform, new Transform(player.Position, Quaternion.Identity, Vector3.One));
        HashSet<ulong> collected = pickupStates.Where(state => state.Lifecycle == LoadingBayPickupLifecycle.Collected).Select(state => state.EntityId).ToHashSet();
        HashSet<ulong> materializedDrops = pickupStates.Where(state => state.Lifecycle == LoadingBayPickupLifecycle.Active && LoadingBayE1M1SemanticCatalog.Pickup(state.EntityId).StartsDormant).Select(state => state.EntityId).ToHashSet();
        foreach (ulong pickupEntityId in materializedDrops)
        {
            LoadingBayE1M1EnemyDefinition owner = LoadingBayE1M1SemanticCatalog.Enemies.Single(enemy => enemy.DropPickupEntityId == pickupEntityId);
            _pickupTranslations[pickupEntityId] = owner.Translation;
            _entities.Set(new EntityId(pickupEntityId), EngineComponentTypes.Transform, new Transform(owner.Translation, Quaternion.Identity, Vector3.One));
        }
        SpatialTriggerReadReceipt before = _spatial.ReadTrigger(new SpatialTriggerReadRequest(_session, _pickups[0].EntityId));
        ulong[] active = _pickups.Where(pickup => (!pickup.StartsDormant || materializedDrops.Contains(pickup.EntityId)) && !collected.Contains(pickup.EntityId))
            .Select(pickup => pickup.EntityId).ToArray();
        SpatialTriggerRestoreReceipt restored = _spatial.RestoreTriggers(new SpatialTriggerRestoreRequest(
            _session, before.Revision, active, ProjectCurrentColliders(player.Position)));
        return _pickups.Select(pickup => new CanonicalPickupTriggerStateFact(
            pickup.EntityId, active.Contains(pickup.EntityId), restored.RevisionBefore, restored.RevisionAfter,
            restored.ActiveOverlapCount, "snapshot.restored")).ToArray();
    }

    internal CanonicalPickupTriggerStateFact MaterializeEnemyDrop(ulong pickupEntityId, Vector3 translation, ulong tick)
    {
        ThrowIfDisposed();
        LoadingBayE1M1PickupPlacement pickup = LoadingBayE1M1SemanticCatalog.Pickup(pickupEntityId);
        if (!pickup.StartsDormant) throw new InvalidOperationException("Only canonical enemy drops may be materialized by enemy defeat.");
        _entities.Set(new EntityId(pickupEntityId), EngineComponentTypes.Transform, new Transform(translation, Quaternion.Identity, Vector3.One));
        _pickupTranslations[pickupEntityId] = translation;
        SpatialTriggerReadReceipt before = _spatial.ReadTrigger(new SpatialTriggerReadRequest(_session, pickupEntityId));
        SpatialTriggerLifecycleReceipt activated = _spatial.SetTriggerActive(new SpatialTriggerSetActiveRequest(_session, pickupEntityId, before.Revision, true, tick));
        return new CanonicalPickupTriggerStateFact(pickupEntityId, activated.Active, activated.RevisionBefore, activated.RevisionAfter, activated.RemovedOverlapCount, "enemy.drop-materialized");
    }

    private SpatialTriggerLifecycleReceipt Reactivate(ulong trigger, ulong expectedRevision, ulong tick, Exception settlementFailure)
    {
        try
        {
            return _spatial.SetTriggerActive(new SpatialTriggerSetActiveRequest(
                _session, trigger, expectedRevision, true, tick));
        }
        catch (Exception compensationFailure)
        {
            throw new AggregateException(settlementFailure, compensationFailure);
        }
    }

    private ReadOnlyMemory<SpatialEntityCollider> ProjectCurrentColliders(Vector3 playerPosition) => _pickups
        .Select(pickup => new SpatialEntityCollider(pickup.EntityId, _pickupTranslations[pickup.EntityId] + pickup.BoundsMin, _pickupTranslations[pickup.EntityId] + pickup.BoundsMax, 0, 0, false, true, true))
        .Append(new SpatialEntityCollider(_player.Value, playerPosition - _playerHalfExtents, playerPosition + _playerHalfExtents, 1, uint.MaxValue, true, false, false))
        .ToArray();

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
    }

    private void ThrowIfDisposed()
    {
        if (_disposed) throw new ObjectDisposedException(nameof(LoadingBaySemanticPickupCoordinator));
    }
}

/// <summary>
/// Engine retains overlap truth for authored world regions; this adapter forwards only canonical enter facts to typed product policy.
/// It intentionally contains neither cooldowns nor world state, so restore never synthesizes an Enter.
/// </summary>
internal sealed class LoadingBayWorldInteractionCoordinator : IDisposable
{
    private const string TriggerScope = "loading-bay.e1m1.world";
    private readonly EntityWorld _entities;
    private readonly SpatialEntityWorld _spatialEntities;
    private readonly ISpatialService _spatial;
    private readonly IPerceptionService _perception;
    private readonly KinematicEntityWorld _kinematics;
    private readonly SpatialSession _session;
    private readonly EntityId _player;
    private readonly Vector3 _playerHalfExtents;
    private readonly HashSet<ulong> _hazards;
    private readonly HashSet<ulong> _floors;
    private readonly HashSet<ulong> _lifts;
    private readonly HashSet<ulong> _secrets;
    private readonly HashSet<ulong> _platforms;
    private readonly int _maximumEntities;
    private bool _disposed;

    internal LoadingBayWorldInteractionCoordinator(IEngineContext engine, LoadingBayPlayerScene player, EntityWorld entities, EntityId playerEntity, LoadingBayTuning tuning)
    {
        _entities = entities; _spatial = engine.Spatial; _perception = engine.Perception; _session = player.Session; _player = playerEntity; _playerHalfExtents = tuning.PlayerPickupHalfExtents;
        _kinematics = new KinematicEntityWorld(_entities, engine.Kinematic, EngineComponentTypes.SpatialCollider);
        _maximumEntities = checked((int)tuning.MaximumSpatialEntityBindings);
        _hazards = LoadingBayE1M1SemanticCatalog.Hazards.Select(value => value.EntityId).ToHashSet();
        _floors = LoadingBayE1M1SemanticCatalog.Floors.Select(value => value.EntityId).ToHashSet();
        _lifts = LoadingBayE1M1SemanticCatalog.Lifts.Select(value => value.EntityId).ToHashSet();
        _secrets = LoadingBayE1M1SemanticCatalog.Secrets.Select(value => value.EntityId).ToHashSet();
        _platforms = LoadingBayE1M1SemanticCatalog.Doors.Select(value => value.EntityId)
            .Concat(LoadingBayE1M1SemanticCatalog.Floors.Select(value => value.PlatformEntityId))
            .Concat(LoadingBayE1M1SemanticCatalog.Lifts.Select(value => value.PlatformEntityId))
            .ToHashSet();
        foreach (LoadingBayE1M1HazardDefinition value in LoadingBayE1M1SemanticCatalog.Hazards) Bind(value.EntityId, value.Translation, value.BoundsMin, value.BoundsMax, "hazard");
        foreach (LoadingBayE1M1FloorDefinition value in LoadingBayE1M1SemanticCatalog.Floors)
        {
            // The authored activation region lives on the activator entity, while motion is delegated to its target platform.
            Bind(value.EntityId, value.ActivationTranslation, value.BoundsMin, value.BoundsMax, "floor");
        }
        foreach (LoadingBayE1M1LiftDefinition value in LoadingBayE1M1SemanticCatalog.Lifts)
        {
            Bind(value.EntityId, value.ActivationTranslation, value.BoundsMin, value.BoundsMax, "lift");
        }
        foreach (LoadingBayE1M1SecretDefinition value in LoadingBayE1M1SemanticCatalog.Secrets) Bind(value.EntityId, value.Translation, value.BoundsMin, value.BoundsMax, "secret");
        foreach (LoadingBayE1M1DoorDefinition value in LoadingBayE1M1SemanticCatalog.Doors) BindPlatform(value.EntityId, value.ClosedTranslation, value.BoundsMin, value.BoundsMax);
        foreach (LoadingBayE1M1FloorDefinition value in LoadingBayE1M1SemanticCatalog.Floors) BindPlatform(value.PlatformEntityId, value.UpperTranslation, value.PlatformBoundsMin, value.PlatformBoundsMax);
        foreach (LoadingBayE1M1LiftDefinition value in LoadingBayE1M1SemanticCatalog.Lifts) BindPlatform(value.PlatformEntityId, value.RaisedTranslation, value.PlatformBoundsMin, value.PlatformBoundsMax);
        _entities.Set(_player, EngineComponentTypes.Transform, new Transform(player.Position, Quaternion.Identity, Vector3.One));
        _entities.Set(_player, EngineComponentTypes.SpatialCollider, new SpatialCollider(-_playerHalfExtents, _playerHalfExtents, 1, uint.MaxValue, true, false, false));
        _spatialEntities = new SpatialEntityWorld(_entities, _spatial, _session, EngineComponentTypes.SpatialCollider);
    }

    /// <summary>
    /// Runs the Engine-owned kinematic phase before the character proposal, then supplies the
    /// current transform for an existing Engine support continuation. It owns no second
    /// support geometry or carry displacement; those remain the character service's work.
    /// </summary>
    internal LoadingBayCharacterStepEnvironment PrepareCharacterStep(
        ulong tick,
        float fixedDeltaSeconds,
        LoadingBayWorldSnapshot world,
        bool supportPresent,
        ulong supportEntity)
    {
        ThrowIfDisposed();
        RealizeMotion(tick, fixedDeltaSeconds, world);
        return new LoadingBayCharacterStepEnvironment(
            ResolvePlatformSupport(supportPresent, supportEntity, _platforms, _entities),
            ProjectPlatformObstacles(_platforms, _entities));
    }

    /// <summary>Forms only the public Engine continuation context after the platform phase has published.</summary>
    internal static CharacterSupport ResolvePlatformSupport(
        bool supportPresent,
        ulong supportEntity,
        IReadOnlySet<ulong> platforms,
        EntityWorld entities)
    {
        ArgumentNullException.ThrowIfNull(platforms);
        ArgumentNullException.ThrowIfNull(entities);
        if (!supportPresent) return default;
        if (!platforms.Contains(supportEntity)
            || !entities.TryGet(new EntityId(supportEntity), EngineComponentTypes.Transform, out Transform transform))
        {
            throw new InvalidOperationException($"E1M1 character continuation referenced unknown platform {supportEntity}.");
        }
        return new CharacterSupport(true, CharacterSupportLifecycle.Active, supportEntity, transform);
    }

    /// <summary>
    /// Borrows the existing canonical Kinematic and SpatialCollider values for one Engine
    /// proposal. This establishes a support identity on landing; it does not introduce a
    /// second geometry representation or a product-owned carry calculation.
    /// </summary>
    internal static CharacterObstacle[] ProjectPlatformObstacles(IReadOnlySet<ulong> platforms, EntityWorld entities)
    {
        ArgumentNullException.ThrowIfNull(platforms);
        ArgumentNullException.ThrowIfNull(entities);
        return platforms.OrderBy(entity => entity).Select(entity =>
        {
            EntityId id = new(entity);
            Transform transform = entities.Get(id, EngineComponentTypes.Transform);
            Kinematic kinematic = entities.Get(id, EngineComponentTypes.Kinematic);
            SpatialCollider collider = entities.Get(id, EngineComponentTypes.SpatialCollider);
            return new CharacterObstacle(entity, transform, collider.Min, collider.Max, collider.Enabled, kinematic.Velocity, Vector3.Zero);
        }).ToArray();
    }

    internal void ReconcileMovementStep(ulong tick, LoadingBayPlayerScene player,
        Func<ulong, ulong, LoadingBayReceipt> applyHazard, Func<ulong, ulong, LoadingBayReceipt> activateFloor,
        Func<ulong, ulong, LoadingBayReceipt> activateLift, Func<ulong, LoadingBayReceipt> discoverSecret, Action<LoadingBayFact> record)
    {
        ThrowIfDisposed(); _entities.Set(_player, EngineComponentTypes.Transform, new Transform(player.Position, Quaternion.Identity, Vector3.One));
        SpatialEntityWorldReconcileReceipt receipt = _spatialEntities.ReconcileTriggers(tick, SpatialTriggerCause.Movement, maximumEntities: _maximumEntities, maximumFactReadback: 64);
        if (receipt.FactsTruncated) throw new InvalidOperationException("E1M1 world trigger facts exceeded the deliberate bound.");
        foreach (SpatialTriggerFactAtReceipt fact in receipt.Facts.Span)
        {
            if (!fact.Present || fact.Subject != _player.Value) continue;
            LoadingBayReceipt result;
            // Facts encode edges only. Continued hazard truth is read below from the active Engine overlap set.
            if (_hazards.Contains(fact.Trigger)) { record(new WorldInteractionFact("hazard-edge", fact.Trigger, fact.Enter ? "enter" : "exit", fact.Tick, 0)); continue; }
            if (!fact.Enter) continue;
            else if (_floors.Contains(fact.Trigger)) result = activateFloor(fact.Trigger, fact.Tick);
            else if (_lifts.Contains(fact.Trigger)) result = activateLift(fact.Trigger, fact.Tick);
            else if (_secrets.Contains(fact.Trigger)) result = discoverSecret(fact.Trigger);
            else continue;
            record(new WorldInteractionFact("trigger", fact.Trigger, result.Code, fact.Tick, 0));
        }
        ReconcileHazardOverlaps(tick, applyHazard, record);
    }

    private void ReconcileHazardOverlaps(ulong tick, Func<ulong, ulong, LoadingBayReceipt> applyHazard, Action<LoadingBayFact> record)
    {
        foreach (ulong trigger in _hazards.OrderBy(value => value))
        {
            SpatialTriggerReadReceipt state = _spatial.ReadTrigger(new SpatialTriggerReadRequest(_session, trigger));
            if (!state.Active) throw new InvalidOperationException($"Canonical E1M1 hazard trigger {trigger} was unexpectedly inactive.");
            if (state.OverlapCount > (uint)_maximumEntities) throw new InvalidOperationException("E1M1 hazard overlap readback exceeded the named shared bound.");
            bool playerOverlapping = false;
            for (uint index = 0; index < state.OverlapCount; index++)
            {
                SpatialTriggerOverlapAtReceipt overlap = _spatial.ReadTriggerOverlapAt(new SpatialTriggerOverlapAtRequest(_session, trigger, index));
                if (!overlap.Present || overlap.Trigger != trigger || overlap.Revision != state.Revision)
                    throw new InvalidOperationException("Engine returned an incoherent E1M1 hazard overlap readback.");
                if (overlap.Subject == _player.Value) playerOverlapping = true;
            }
            if (!playerOverlapping) continue;
            LoadingBayReceipt outcome = applyHazard(trigger, tick);
            record(new WorldInteractionFact("hazard-overlap", trigger, outcome.Code, tick, 0));
        }
    }

    /// <summary>One bounded Engine perception query chooses the authored use target; product policy decides only what that target means.</summary>
    internal void Use(ulong tick, LoadingBayPlayerScene player, Func<ulong, ulong, LoadingBayReceipt> activateDoor, Func<ulong, LoadingBayReceipt> completeExit, Action<LoadingBayFact> record)
    {
        ThrowIfDisposed();
        LoadingBayE1M1DoorDefinition[] doors = LoadingBayE1M1SemanticCatalog.Doors;
        LoadingBayE1M1ExitDefinition[] exits = LoadingBayE1M1SemanticCatalog.Exits;
        PerceptionTarget[] targets = doors.Select(value => new PerceptionTarget(value.EntityId, value.ClosedTranslation))
            .Concat(exits.Select(value => new PerceptionTarget(value.EntityId, value.Translation))).ToArray();
        PerceptionReadoutLeaseReceipt receipt = _perception.QueryVisibility(new PerceptionQueryRequest(
            _session,
            new[] { player.CreatePerceptionObserver(_player.Value, LoadingBayTuning.E1M1) },
            targets,
            ReadOnlyMemory<SpatialEntityCollider>.Empty,
            0,
            0,
            64));
        if (receipt.SelectedObservers != 1 || receipt.SelectedTargets != targets.Length || receipt.Pairs.Length != targets.Length) throw new InvalidOperationException("Engine Perception returned an incomplete E1M1 use query.");
        PerceptionPair[] candidates = receipt.Pairs.ToArray().Where(pair => pair.Kind == PerceptionPairKind.Visible)
            .Where(pair => doors.Any(door => door.EntityId == pair.Target && pair.Distance <= door.ActivationRadius) || exits.Any(exit => exit.EntityId == pair.Target && pair.Distance <= exit.ActivationRadius))
            .OrderBy(pair => pair.Distance).ToArray();
        if (candidates.Length == 0) { record(new WorldInteractionFact("use", 0, "unavailable", tick, 0)); return; }
        PerceptionPair selected = candidates[0];
        LoadingBayReceipt result = doors.Any(door => door.EntityId == selected.Target)
            ? activateDoor(selected.Target, tick)
            : completeExit(selected.Target);
        record(new WorldInteractionFact("use", selected.Target, result.Code, tick, 0));
    }

    /// <summary>Environment occlusion is resolved by Engine Spatial; the tiny epsilon prevents the source barrel from self-blocking its own ray.</summary>
    internal bool BarrelOccluded(LoadingBayE1M1BarrelDefinition source, LoadingBayE1M1BarrelDefinition target)
    {
        const float SourceEpsilon = 0.03125f;
        Vector3 delta = target.Translation - source.Translation;
        if (delta.LengthSquared() <= SourceEpsilon * SourceEpsilon) return false;
        Vector3 direction = Vector3.Normalize(delta);
        Vector3 start = source.Translation + direction * SourceEpsilon;
        SpatialHit hit = _spatial.CastSegment(new SpatialSegmentCastRequest(_session, start, target.Translation,
            new SpatialQueryFilter(0, uint.MaxValue), ReadOnlyMemory<SpatialEntityCollider>.Empty, new[] { source.EntityId, target.EntityId }, ReadOnlyMemory<SpatialEntityCollider>.Empty));
        return hit.Present && hit.Kind is SpatialHitKind.Voxel or SpatialHitKind.StaticMesh;
    }

    private void RealizeMotion(ulong tick, float deltaSeconds, LoadingBayWorldSnapshot world)
    {
        if (!float.IsFinite(deltaSeconds) || deltaSeconds <= 0f) return;
        List<EntityId> active = [];
        foreach (LoadingBayDoorSnapshot state in world.Doors.Where(value => value.State is LoadingBayDoorState.Opening or LoadingBayDoorState.Closing))
        {
            LoadingBayE1M1DoorDefinition definition = LoadingBayE1M1SemanticCatalog.Doors.Single(value => value.EntityId == state.EntityId);
            SetVelocity(new EntityId(state.EntityId), state.State == LoadingBayDoorState.Opening ? definition.OpenTranslation : definition.ClosedTranslation, state.DueStep, tick, deltaSeconds, active);
        }
        foreach (LoadingBayFloorSnapshot state in world.Floors.Where(value => value.State == LoadingBayFloorState.Lowering))
        {
            LoadingBayE1M1FloorDefinition definition = LoadingBayE1M1SemanticCatalog.Floors.Single(value => value.EntityId == state.EntityId);
            SetVelocity(new EntityId(definition.PlatformEntityId), definition.LoweredTranslation, state.DueStep, tick, deltaSeconds, active);
        }
        foreach (LoadingBayLiftSnapshot state in world.Lifts.Where(value => value.State is LoadingBayLiftState.Lowering or LoadingBayLiftState.Raising))
        {
            LoadingBayE1M1LiftDefinition definition = LoadingBayE1M1SemanticCatalog.Lifts.Single(value => value.EntityId == state.EntityId);
            SetVelocity(new EntityId(definition.PlatformEntityId), state.State == LoadingBayLiftState.Lowering ? definition.LoweredTranslation : definition.RaisedTranslation, state.DueStep, tick, deltaSeconds, active);
        }
        if (active.Count != 0) _kinematics.Prepare(_session, deltaSeconds, 6, active.ToArray()).Apply();
    }

    internal void RestoreMotion(LoadingBayWorldSnapshot world, ulong tick)
    {
        ThrowIfDisposed();
        foreach (LoadingBayDoorSnapshot state in world.Doors)
        {
            LoadingBayE1M1DoorDefinition definition = LoadingBayE1M1SemanticCatalog.Doors.Single(value => value.EntityId == state.EntityId);
            float progress = state.State switch
            {
                LoadingBayDoorState.Open => 1f,
                LoadingBayDoorState.Closed => 0f,
                LoadingBayDoorState.Opening => Progress(state.DueStep, tick, definition.MotionDurationTicks),
                LoadingBayDoorState.Closing => 1f - Progress(state.DueStep, tick, definition.MotionDurationTicks),
                _ => 0f,
            };
            RestorePlatform(new EntityId(state.EntityId), Vector3.Lerp(definition.ClosedTranslation, definition.OpenTranslation, progress));
        }
        foreach (LoadingBayFloorSnapshot state in world.Floors)
        {
            LoadingBayE1M1FloorDefinition definition = LoadingBayE1M1SemanticCatalog.Floors.Single(value => value.EntityId == state.EntityId);
            float progress = state.State == LoadingBayFloorState.Lowered ? 1f : state.State == LoadingBayFloorState.Lowering ? Progress(state.DueStep, tick, definition.MotionDurationTicks) : 0f;
            RestorePlatform(new EntityId(definition.PlatformEntityId), Vector3.Lerp(definition.UpperTranslation, definition.LoweredTranslation, progress));
        }
        foreach (LoadingBayLiftSnapshot state in world.Lifts)
        {
            LoadingBayE1M1LiftDefinition definition = LoadingBayE1M1SemanticCatalog.Lifts.Single(value => value.EntityId == state.EntityId);
            float progress = state.State switch
            {
                LoadingBayLiftState.Raised => 0f,
                LoadingBayLiftState.Waiting => 1f,
                LoadingBayLiftState.Lowering => Progress(state.DueStep, tick, definition.MotionDurationTicks),
                LoadingBayLiftState.Raising => 1f - Progress(state.DueStep, tick, definition.MotionDurationTicks),
                _ => 0f,
            };
            RestorePlatform(new EntityId(definition.PlatformEntityId), Vector3.Lerp(definition.RaisedTranslation, definition.LoweredTranslation, progress));
        }
    }

    private static float Progress(ulong dueStep, ulong tick, int duration) => duration <= 0 ? 1f : Math.Clamp(1f - Math.Max(0, dueStep > tick ? dueStep - tick : 0) / (float)duration, 0f, 1f);
    private void RestorePlatform(EntityId entity, Vector3 translation)
    {
        Transform current = _entities.Get(entity, EngineComponentTypes.Transform);
        _entities.Set(entity, EngineComponentTypes.Transform, current with { Translation = translation });
        Kinematic kinematic = _entities.Get(entity, EngineComponentTypes.Kinematic);
        _entities.Set(entity, EngineComponentTypes.Kinematic, new Kinematic(kinematic.HalfExtents, Vector3.Zero));
    }

    private void SetVelocity(EntityId entity, Vector3 destination, ulong dueStep, ulong tick, float deltaSeconds, List<EntityId> active)
    {
        if (dueStep <= tick) return;
        Transform current = _entities.Get(entity, EngineComponentTypes.Transform);
        ulong remainingSteps = dueStep - tick + 1;
        Vector3 velocity = (destination - current.Translation) / (remainingSteps * deltaSeconds);
        Kinematic currentKinematic = _entities.Get(entity, EngineComponentTypes.Kinematic);
        _entities.Set(entity, EngineComponentTypes.Kinematic, new Kinematic(currentKinematic.HalfExtents, velocity));
        active.Add(entity);
    }

    private void Bind(ulong entityId, Vector3 translation, Vector3 min, Vector3 max, string tag)
    {
        _entities.Set(new EntityId(entityId), EngineComponentTypes.Transform, new Transform(translation, Quaternion.Identity, Vector3.One));
        _entities.Set(new EntityId(entityId), EngineComponentTypes.SpatialCollider, new SpatialCollider(min, max, 0, 0, false, true, true));
        _spatial.RegisterTrigger(new SpatialTriggerRegisterRequest(_session, entityId, TriggerScope, tag, SpatialTriggerGeometry.EntityBounds));
    }

    private void BindPlatform(ulong entityId, Vector3 translation, Vector3 boundsMin, Vector3 boundsMax)
    {
        Vector3 halfExtents = Vector3.Max(Vector3.Abs(boundsMin), Vector3.Abs(boundsMax));
        _entities.Set(new EntityId(entityId), EngineComponentTypes.Transform, new Transform(translation, Quaternion.Identity, Vector3.One));
        _entities.Set(new EntityId(entityId), EngineComponentTypes.Kinematic, new Kinematic(halfExtents, Vector3.Zero));
        _entities.Set(new EntityId(entityId), EngineComponentTypes.SpatialCollider, new SpatialCollider(boundsMin, boundsMax, 2, uint.MaxValue, true, false, false));
    }

    public void Dispose() => _disposed = true;
    private void ThrowIfDisposed() { if (_disposed) throw new ObjectDisposedException(nameof(LoadingBayWorldInteractionCoordinator)); }
}
