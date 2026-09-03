using System.Text;
using Rusty.Engine;

namespace LoadingBay.Game;

/// <summary>Engine UI transport projection; the browser may realize it as a disposable HUD.</summary>
internal sealed class LoadingBayHudProjection : IDisposable
{
    private readonly IUiService _ui;
    private readonly UiStream _stream;
    private ulong _sequence;
    private bool _disposed;

    internal LoadingBayHudProjection(IUiService ui)
    {
        _ui = ui ?? throw new ArgumentNullException(nameof(ui));
        _stream = _ui.OpenStream(new UiStreamRequest("loading-bay.hud", "loading-bay.hud.snapshot.v1"));
    }

    internal void Publish(LoadingBayReadout readout, string projectPath, string voxelPath, LoadingBayEngineServiceReadout services)
    {
        if (_disposed) return;
        LoadingBayUiValueBuilder value = new();
        // Facts are bounded at the product journal and remain typed copies across the UI boundary.
        uint facts = value.Array(readout.Facts.Select(fact => Fact(value, fact)).ToArray());
        uint pickups = value.Object(
            ("total", value.Number(readout.Pickups.Length)),
            ("active", value.Number(readout.Pickups.Count(pickup => pickup.Lifecycle == LoadingBayPickupLifecycle.Active))),
            ("dormant", value.Number(readout.Pickups.Count(pickup => pickup.Lifecycle == LoadingBayPickupLifecycle.Dormant))),
            ("collected", value.Number(readout.Pickups.Count(pickup => pickup.Lifecycle == LoadingBayPickupLifecycle.Collected))));
        uint cooldowns = value.Array(readout.WeaponCooldowns.Select(cooldown => value.Object(
            ("weapon", value.String(cooldown.WeaponId)), ("readyAtTick", value.Number(cooldown.ReadyAtTick)))).ToArray());
        uint player = value.Object(
            ("x", value.Number(readout.PlayerState.Position.X)), ("y", value.Number(readout.PlayerState.Position.Y)), ("z", value.Number(readout.PlayerState.Position.Z)),
            ("yawRadians", value.Number(readout.PlayerState.Look.YawRadians)), ("pitchRadians", value.Number(readout.PlayerState.Look.PitchRadians)),
            ("continuation", value.Bool(readout.PlayerState.Continuation is not null)),
            ("grounded", value.Bool(readout.PlayerState.Continuation?.Motion.Grounded ?? false)), ("stance", value.String(readout.PlayerState.Continuation?.Motion.Stance.ToString() ?? "PreFirstStep")),
            ("commandSequence", value.Number(readout.PlayerState.Continuation?.Motion.LastCommandSequence ?? 0)),
            ("continuationGeneration", value.Number(readout.PlayerState.Continuation?.SourceGeneration ?? 0)));
        uint catalog = value.Object(
            ("path", value.String(services.VoxelScene.CatalogPath)),
            ("canonicalHash", value.String(services.VoxelScene.CatalogHash)),
            ("entryCount", value.Number(services.VoxelScene.CatalogEntryCount)),
            ("programs", value.Number(LoadingBayE1M1SemanticCatalog.ProgramDescriptors.Length)),
            ("pickups", value.Number(LoadingBayE1M1SemanticCatalog.Pickups.Length)),
            ("enemies", value.Number(LoadingBayE1M1SemanticCatalog.Enemies.Length)),
            ("encounters", value.Number(LoadingBayE1M1SemanticCatalog.Encounters.Length)),
            ("doors", value.Number(LoadingBayE1M1SemanticCatalog.Doors.Length)),
            ("hazards", value.Number(LoadingBayE1M1SemanticCatalog.Hazards.Length)),
            ("materialCount", value.Number(services.VoxelScene.MaterialCount)),
            ("boundMaterialCount", value.Number(services.VoxelScene.BoundMaterialCount)),
            ("mappingCount", value.Number(services.VoxelScene.MappingCount)),
            ("mappingSourceRevision", value.Number(services.VoxelScene.MappingSourceRevision)),
            ("mappingMeshRevision", value.Number(services.VoxelScene.MappingMeshRevision)),
            ("sceneSourceRevision", value.Number(services.VoxelScene.SceneSourceRevision)),
            ("sceneMeshRevision", value.Number(services.VoxelScene.SceneMeshRevision)),
            ("sceneChunkCount", value.Number(services.VoxelScene.SceneChunkCount)),
            ("realized", value.Bool(services.VoxelScene.Realized)));
        uint materials = value.Array(services.VoxelScene.Materials.Select(material => value.Object(
            ("id", value.String(material.MaterialId)),
            ("hash", value.String(material.MaterialHash)),
            ("slot", value.Number(material.MaterialSlot)),
            ("textureId", value.String(material.TextureId)),
            ("textureHash", value.String(material.TextureHash)))).ToArray());
        uint sky = value.Object(
            ("path", value.String(services.Sky.SourcePath)),
            ("hash", value.String(services.Sky.SourceHash.ToString())),
            ("byteLength", value.Number(services.Sky.SourceByteLength)),
            ("resourceHandle", value.Number(services.Sky.ResourceHandle)),
            ("resourceRealized", value.Bool(services.Sky.ResourceRealized)),
            ("backgroundSelected", value.Bool(services.Sky.BackgroundSelected)));
        uint programs = value.Array(LoadingBayE1M1SemanticCatalog.ProgramDescriptors.Select(program => value.Object(
            ("id", value.String(program.Id)), ("family", value.String(program.Family.ToString())), ("sourceIndex", value.Number(program.SourceIndex)), ("bindingShape", value.String(program.BindingShape.ToString())))).ToArray());
        uint pickupBindings = value.Array(readout.Pickups.Select(pickup => value.Object(
            ("entityId", value.Number(pickup.EntityId)), ("item", value.String(pickup.ItemId)), ("program", value.String(pickup.ProgramId)), ("lifecycle", value.String(pickup.Lifecycle.ToString())), ("cause", value.String(pickup.Cause)), ("tick", value.Number(pickup.Tick)), ("triggerRevision", value.Number(pickup.TriggerRevision)))).ToArray());
        uint enemyBindings = value.Array(readout.Enemies.Select(enemy => value.Object(
            ("entityId", value.Number(enemy.EntityId)), ("label", value.String(enemy.Label)), ("health", value.Number(enemy.Health)), ("posture", value.String(enemy.Posture.ToString())), ("visible", value.Bool(enemy.Visible)), ("readyAtTick", value.Number(enemy.ReadyAtTick)), ("dropPickupEntityId", value.Number(enemy.DropPickupEntityId)))).ToArray());
        uint tuning = value.Object(
            ("movementSpeed", value.Number(readout.Tuning.MovementSpeed)),
            ("lookDegreesPerUnit", value.Number(readout.Tuning.LookDegreesPerUnit)),
            ("gravity", value.Number(readout.Tuning.Gravity)),
            ("jumpSpeed", value.Number(readout.Tuning.JumpSpeed)),
            ("authoredPlayerX", value.Number(readout.Tuning.AuthoredPlayerPosition.X)),
            ("authoredPlayerY", value.Number(readout.Tuning.AuthoredPlayerPosition.Y)),
            ("authoredPlayerZ", value.Number(readout.Tuning.AuthoredPlayerPosition.Z)),
            ("authoredPlayerKinematicHalfHeight", value.Number(readout.Tuning.AuthoredPlayerKinematicHalfHeight)),
            ("standingCharacterHeight", value.Number(readout.Tuning.StandingCharacterHeight)),
            ("crouchedCharacterHeight", value.Number(readout.Tuning.CrouchedCharacterHeight)),
            ("characterRadius", value.Number(readout.Tuning.CharacterRadius)),
            ("authoredBaseEyeHeight", value.Number(readout.Tuning.AuthoredBaseEyeHeight)),
            ("engineCenterLift", value.Number(readout.Tuning.EngineCenterLift)),
            ("initialEngineCenterX", value.Number(readout.Tuning.InitialEngineCenter.X)),
            ("initialEngineCenterY", value.Number(readout.Tuning.InitialEngineCenter.Y)),
            ("initialEngineCenterZ", value.Number(readout.Tuning.InitialEngineCenter.Z)),
            ("eyeOffsetFromCenter", value.Number(readout.Tuning.EyeOffsetFromCenter)),
            ("maximumHealth", value.Number(readout.Tuning.MaximumHealth)),
            ("maximumPickupBindings", value.Number(readout.Tuning.MaximumPickupBindings)),
            ("maximumPickupFactReadback", value.Number(readout.Tuning.MaximumPickupFactReadback)),
            ("maximumSpatialEntityBindings", value.Number(readout.Tuning.MaximumSpatialEntityBindings)));
        uint root = value.Object(
            ("content", value.String(readout.Tuning.ContentIdentity)),
            ("projectPath", value.String(projectPath)),
            ("voxelPath", value.String(voxelPath)),
            ("catalogHash", value.String(services.VoxelScene.CatalogHash)),
            ("materialCount", value.Number(services.VoxelScene.MaterialCount)),
            ("materialMappingCount", value.Number(services.VoxelScene.MappingCount)),
            ("voxelPresentationRealized", value.Bool(services.VoxelScene.Realized)),
            ("skyPath", value.String(services.Sky.SourcePath)),
            ("skyHash", value.String(services.Sky.SourceHash.ToString())),
            ("skyResourceRealized", value.Bool(services.Sky.ResourceRealized)),
            ("skyBackgroundSelected", value.Bool(services.Sky.BackgroundSelected)),
            ("health", value.Number(readout.Health)),
            ("armor", value.Number(readout.Armor)),
            ("armorProtection", value.String(readout.ArmorProtection.Mode.ToString())),
            ("armorAbsorptionDivisor", value.Number(readout.ArmorProtection.AbsorptionDivisor)),
            ("bullets", value.Number(readout.Bullets)),
            ("shells", value.Number(readout.Shells)),
            ("enemyCount", value.Number(readout.Enemies.Length)),
            ("defeatedEnemies", value.Number(readout.Enemies.Count(enemy => enemy.Posture == LoadingBayEnemyPosture.Defeated))),
            ("generation", value.Number(readout.Generation)),
            ("step", value.Number(readout.Step)),
            ("updateMode", value.String(readout.UpdateFacts.Mode.ToString())),
            ("lifecycle", value.String(readout.UpdateFacts.LifecycleState.ToString())),
            ("controlRevision", value.Number(readout.UpdateFacts.ControlRevision)),
            ("observedHostTimeNanoseconds", value.Number(readout.UpdateFacts.ObservedHostTimeNanoseconds)),
            ("fixedStepHz", value.Number(readout.UpdateFacts.FixedStepHz)),
            ("admittedSteps", value.Number(readout.UpdateFacts.AdmittedStepCount)),
            ("droppedSteps", value.Number(readout.UpdateFacts.DroppedStepCount)),
            ("fixedDeltaSeconds", value.Number(readout.UpdateFacts.FixedDeltaSeconds)),
            ("complete", value.Bool(readout.Complete)),
            ("pickups", pickups),
            ("catalog", catalog),
            ("programBindings", programs),
            ("pickupBindings", pickupBindings),
            ("enemyBindings", enemyBindings),
            ("materials", materials),
            ("sky", sky),
            ("weaponCooldowns", cooldowns),
            ("player", player),
            ("pendingSchedules", value.Number(readout.PendingSchedules)),
            ("droppedFacts", value.Number(readout.DroppedFacts)),
            ("facts", facts),
            ("tuning", tuning),
            ("exitVisibility", value.Bool(services.Perception.Visible)),
            ("exitVisibilityRevision", value.Number(services.Perception.Revision)),
            ("exitVisibilityCasts", value.Number(services.Perception.VisibilityCasts)),
            ("exitOcclusionRejects", value.Number(services.Perception.OcclusionRejects)),
            ("presentationBillboards", value.Number(services.Presentation.ActiveBillboards)),
            ("animationCue", value.String(services.Animation.CueId)),
            ("animationRetainedAppearance", value.Bool(services.Animation.RetainedAppearance)),
            ("animationCompletionObserved", value.Bool(services.Animation.CompletionObserved)),
            ("animationCompletionTransitions", value.Number(services.Animation.CompletionTransitionCount)),
            ("animationAdmittedMeshes", value.Number(services.Animation.AdmittedMeshes)),
            ("animationRetainedInstances", value.Number(services.Animation.RetainedInstances)),
            ("animationPendingPlayback", value.Number(services.Animation.PendingPlaybackCommands)),
            ("animationRetainedFacts", value.Number(services.Animation.RetainedRealizationFacts)),
            ("animationEvictedFacts", value.Number(services.Animation.EvictedRealizationFacts)),
            ("effectsVolume", value.Number(services.Audio.Volume)),
            ("effectsMuted", value.Bool(services.Audio.Muted)));
        _ui.PublishProjection(new UiProjection(_stream, ++_sequence, value.Build(root)));
    }

    private static uint Fact(LoadingBayUiValueBuilder value, LoadingBayFact fact) => fact switch
    {
        SessionStartedFact f => value.Object(("kind", value.String("session.started")), ("player", value.Number(f.Player.Value))),
        PickupCollectedFact f => value.Object(("kind", value.String("pickup.collected")), ("pickup", value.String(f.Pickup)), ("item", value.String(f.Item)), ("quantity", value.Number(f.Quantity))),
        DamageAppliedFact f => value.Object(("kind", value.String("damage.applied")), ("target", value.String(f.Target)), ("requested", value.Number(f.Requested)), ("armorAbsorbed", value.Number(f.ArmorAbsorbed)), ("healthApplied", value.Number(f.HealthApplied)), ("cause", value.String(f.Cause)), ("defeated", value.Bool(f.Defeated))),
        DoorChangedFact f => value.Object(("kind", value.String("door.changed")), ("door", value.String(f.Door)), ("open", value.Bool(f.Open))),
        EncounterChangedFact f => value.Object(("kind", value.String("encounter.changed")), ("encounter", value.String(f.Encounter)), ("cleared", value.Bool(f.Cleared))),
        WorldActionFact f => value.Object(("kind", value.String("world.action")), ("code", value.String(f.Code)), ("subject", value.String(f.Subject))),
        SecretDiscoveredFact f => value.Object(("kind", value.String("secret.discovered")), ("secret", value.String(f.Secret))),
        ExitCompletedFact f => value.Object(("kind", value.String("exit.completed")), ("exit", value.String(f.Exit))),
        DeveloperTrackChangedFact f => value.Object(("kind", value.String("developer.track")), ("track", value.String(f.Track)), ("value", value.Number(f.Value)), ("correlation", value.String(f.Correlation))),
        SnapshotRestoredFact f => value.Object(("kind", value.String("snapshot.restored")), ("identity", value.String(f.Identity))),
        SemanticInputFact f => value.Object(("kind", value.String("input.semantic")), ("intent", value.String(f.Intent))),
        CanonicalPickupOverlapFact f => value.Object(("kind", value.String("pickup.overlap")), ("pickupEntityId", value.Number(f.PickupEntityId)), ("subjectEntityId", value.Number(f.SubjectEntityId)), ("tick", value.Number(f.Tick)), ("accepted", value.Bool(f.Accepted)), ("code", value.String(f.Code))),
        CanonicalPickupTriggerStateFact f => value.Object(("kind", value.String("pickup.trigger")), ("pickupEntityId", value.Number(f.PickupEntityId)), ("active", value.Bool(f.Active)), ("revisionBefore", value.Number(f.RevisionBefore)), ("revisionAfter", value.Number(f.RevisionAfter)), ("overlapCount", value.Number(f.OverlapCount)), ("cause", value.String(f.Cause))),
        PickupLoadoutChangedFact f => value.Object(("kind", value.String("pickup.loadout")), ("pickupEntityId", value.Number(f.PickupEntityId)), ("item", value.String(f.ItemId)), ("program", value.String(f.ProgramId)), ("active", value.Bool(f.Active)), ("code", value.String(f.Code))),
        PickupLifecycleFact f => value.Object(("kind", value.String("pickup.lifecycle")), ("pickupEntityId", value.Number(f.PickupEntityId)), ("item", value.String(f.ItemId)), ("program", value.String(f.ProgramId)), ("lifecycle", value.String(f.Lifecycle.ToString())), ("cause", value.String(f.Cause)), ("tick", value.Number(f.Tick)), ("triggerRevision", value.Number(f.TriggerRevision))),
        EnemyPostureChangedFact f => value.Object(("kind", value.String("enemy.posture")), ("enemyEntityId", value.Number(f.EnemyEntityId)), ("posture", value.String(f.Posture.ToString())), ("health", value.Number(f.Health)), ("tick", value.Number(f.Tick)), ("cause", value.String(f.Cause))),
        EnemyHitFact f => value.Object(("kind", value.String("enemy.hit")), ("enemyEntityId", value.Number(f.EnemyEntityId)), ("weapon", value.String(f.WeaponId)), ("damage", value.Number(f.Damage)), ("remainingHealth", value.Number(f.RemainingHealth)), ("tick", value.Number(f.Tick))),
        EnemyDefeatedFact f => value.Object(("kind", value.String("enemy.defeated")), ("enemyEntityId", value.Number(f.EnemyEntityId)), ("dropPickupEntityId", value.Number(f.DropPickupEntityId)), ("tick", value.Number(f.Tick))),
        EnemyPerceptionFact f => value.Object(("kind", value.String("enemy.perception")), ("enemyEntityId", value.Number(f.EnemyEntityId)), ("visible", value.Bool(f.Visible)), ("tick", value.Number(f.Tick)), ("visibilityCasts", value.Number(f.VisibilityCasts)), ("occlusionRejects", value.Number(f.OcclusionRejects))),
        EnemyAttackFact f => value.Object(("kind", value.String("enemy.attack")), ("enemyEntityId", value.Number(f.EnemyEntityId)), ("attackKind", value.String(f.Kind.ToString())), ("hitPlayer", value.Bool(f.HitPlayer)), ("tick", value.Number(f.Tick)), ("cause", value.String(f.Cause))),
        EnemyProjectileFact f => value.Object(("kind", value.String("enemy.projectile")), ("enemyEntityId", value.Number(f.EnemyEntityId)), ("tick", value.Number(f.Tick)), ("cause", value.String(f.Cause))),
        EncounterActivatedFact f => value.Object(("kind", value.String("encounter.activated")), ("encounterEntityId", value.Number(f.EncounterEntityId)), ("encounter", value.String(f.Encounter)), ("radius", value.Number(f.Radius)), ("tick", value.Number(f.Tick))),
        WeaponFiredFact f => value.Object(("kind", value.String("weapon.fired")), ("weapon", value.String(f.WeaponId)), ("tick", value.Number(f.Tick)), ("pelletCount", value.Number(f.PelletCount)), ("impactCount", value.Number(f.ImpactCount))),
        WeaponMissedFact f => value.Object(("kind", value.String("weapon.missed")), ("weapon", value.String(f.WeaponId)), ("tick", value.Number(f.Tick)), ("pelletIndex", value.Number(f.PelletIndex)), ("cause", value.String(f.Cause))),
        RejectedFact f => value.Object(("kind", value.String("rejected")), ("code", value.String(f.Code)), ("correlation", value.String(f.Correlation ?? string.Empty))),
        WorldInteractionFact f => value.Object(("kind", value.String("world.interaction")), ("family", value.String(f.Family)), ("entityId", value.Number(f.EntityId)), ("state", value.String(f.State)), ("tick", value.Number(f.Tick)), ("dueStep", value.Number(f.DueStep))),
        BarrelExplosionFact f => value.Object(("kind", value.String("barrel.exploded")), ("barrelEntityId", value.Number(f.BarrelEntityId)), ("damage", value.Number(f.Damage)), ("radius", value.Number(f.Radius)), ("tick", value.Number(f.Tick)), ("chained", value.Bool(f.Chained))),
        _ => throw new ArgumentOutOfRangeException(nameof(fact), "Loading Bay HUD has no typed fact projection."),
    };

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        _stream.Dispose();
    }
}

/// <summary>Builds copied structured values for the generated Engine UI boundary.</summary>
internal sealed class LoadingBayUiValueBuilder
{
    private readonly List<StructuredValueNode> _nodes = [];
    private readonly List<uint> _edges = [];
    private readonly List<byte> _utf8 = [];

    internal uint Number(double value) => Add(StructuredValueKind.Number, number: value);
    internal uint Bool(bool value) => Add(StructuredValueKind.Bool, boolValue: value ? 1u : 0u);
    internal uint String(string value)
    {
        (uint offset, uint length) = Bytes(value);
        return Add(StructuredValueKind.String, textOffset: offset, textLength: length);
    }
    internal uint Array(uint[] values)
    {
        uint first = checked((uint)_edges.Count);
        foreach (uint value in values) { Validate(value); _edges.Add(value); }
        return Add(StructuredValueKind.Array, first: first, count: checked((uint)values.Length));
    }
    internal uint Object(params (string Key, uint Value)[] fields)
    {
        uint first = checked((uint)_edges.Count);
        foreach ((string key, uint value) in fields)
        {
            Validate(value);
            (uint offset, uint length) = Bytes(key);
            uint keyed = checked((uint)_nodes.Count);
            _nodes.Add(_nodes[checked((int)value)] with { KeyOffset = offset, KeyLen = length });
            _edges.Add(keyed);
        }
        return Add(StructuredValueKind.Object, first: first, count: checked((uint)fields.Length));
    }
    internal UiValue Build(uint root)
    {
        Validate(root);
        return new UiValue(_nodes.ToArray(), _edges.ToArray(), root, _utf8.ToArray());
    }
    private uint Add(StructuredValueKind kind, uint boolValue = 0, double number = 0d, uint textOffset = 0, uint textLength = 0, uint first = 0, uint count = 0)
    {
        uint index = checked((uint)_nodes.Count);
        _nodes.Add(new StructuredValueNode(kind, boolValue, number, 0, 0, textOffset, textLength, first, count));
        return index;
    }
    private (uint Offset, uint Length) Bytes(string value)
    {
        byte[] bytes = Encoding.UTF8.GetBytes(value);
        uint offset = checked((uint)_utf8.Count);
        _utf8.AddRange(bytes);
        return (offset, checked((uint)bytes.Length));
    }
    private void Validate(uint value)
    {
        if (value >= (uint)_nodes.Count) throw new ArgumentOutOfRangeException(nameof(value));
    }
}
