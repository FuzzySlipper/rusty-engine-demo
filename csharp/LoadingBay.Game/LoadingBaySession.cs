using Rusty.Engine;
using Rusty.Engine.Application;
using Rusty.Engine.Entities;
using Rusty.Engine.Persistence;
using Mechanics = Rusty.Engine.Mechanics;

namespace LoadingBay.Game;

/// <summary>Authoritative admitted-step product state plus narrowly owned generated Engine service adapters.</summary>
internal sealed class LoadingBaySession : ILoadingBaySession, ILoadingBayDebugSession
{
    private readonly LoadingBayTuning _tuning;
    private readonly EntityWorld _entities = new([
        EngineComponentTypes.Transform,
        EngineComponentTypes.SpatialCollider,
        EngineComponentTypes.Kinematic,
    ]);
    private readonly Mechanics.InventoryWorld _inventory = new();
    private SimulationScheduler _scheduler = new();
    private readonly LoadingBayWorldState _world = new();
    private ProductStateStore<LoadingBaySnapshot>? _store;
    private LoadingBayEngineServices? _engineServices;
    private IEngineContext? _engineContext;
    private ProductContent? _productContent;
    private LoadingBayExitPresentation? _exitPresentation;
    private LoadingBayExitButtonAnimation? _exitButtonAnimation;
    private LoadingBaySkyReadout _skyReadout;
    private bool _sharedRealizationsActive;
    private readonly Queue<LoadingBayFact> _journal = new();
    private readonly Queue<Exception> _retiredProjectionFailures = new();
    private ulong _droppedRetiredProjectionFailures;
    private readonly Dictionary<string, bool> _doors = new(StringComparer.Ordinal);
    private readonly HashSet<ulong> _activatedEncounters = [];
    // Direct fixture-only collection keys are deliberately separate from canonical E1M1 state.
    // Canonical pickups have exactly one authority: _pickupStates.
    private readonly HashSet<string> _manualPickupKeys = new(StringComparer.Ordinal);
    private readonly Dictionary<ulong, LoadingBayPickupSnapshot> _pickupStates = new();
    private readonly Dictionary<ulong, EnemyState> _actors = new();
    private readonly Dictionary<string, ulong> _weaponReadyAt = new(StringComparer.Ordinal);
    private readonly HashSet<string> _secrets = new(StringComparer.Ordinal);
    private Mechanics.ExactTrack _health;
    private Mechanics.ExactTrack _armor;
    private LoadingBayArmorProtection _armorProtection = LoadingBayArmorProtection.None;
    private LoadingBayPlayerSnapshot _playerSnapshot;
    private readonly EntityId _player;
    private ProductUpdateFacts _facts;
    private bool _hasFacts;
    private bool _complete;
    private bool _disposed;
    private ulong _dropped;
    private Action<EntityWorld>? _debugEntityWorldChanged;

    public LoadingBaySession(LoadingBayTuning? tuning = null)
    {
        _tuning = tuning ?? LoadingBayTuning.E1M1;
        BootstrapCanonicalEntities(_entities);
        _player = new EntityId(1);
        _inventory.RegisterInventory(new Mechanics.InventoryState(_player, [new Mechanics.InventoryCapacityLimit(Mechanics.CapacityMetricId.Parse("loading-bay.inventory.slots"), _tuning.InventorySlots)]));
        _inventory.RegisterEquipment(new Mechanics.EquipmentState(_player));
        _health = Track("health", _tuning.StartingHealth, _tuning.MaximumHealth);
        _armor = Track("armor", _tuning.StartingArmor, _tuning.MaximumArmor);
        _playerSnapshot = InitialPlayerSnapshot(_tuning);
        foreach (LoadingBayE1M1PickupPlacement pickup in LoadingBayE1M1SemanticCatalog.Pickups)
            _pickupStates.Add(pickup.EntityId, new LoadingBayPickupSnapshot(pickup.EntityId, pickup.ItemId, pickup.ProgramId, pickup.StartsDormant ? LoadingBayPickupLifecycle.Dormant : LoadingBayPickupLifecycle.Active, "bootstrap", 0, 0));
        foreach (LoadingBayE1M1EnemyDefinition enemy in LoadingBayE1M1SemanticCatalog.Enemies)
            _actors.Add(enemy.EntityId, new EnemyState(enemy.MaximumHealth, LoadingBayEnemyPosture.Dormant, 0));
        ApplyPlayerSetup(LoadingBayE1M1SemanticCatalog.PlayerSetup("player/e1m1-pistol-start"));
        Record(new SessionStartedFact(_player));
    }

    public LoadingBaySession(
        IEngineContext engine,
        ProductContent content,
        LoadingBayExitPresentation exitPresentation,
        LoadingBayExitButtonAnimation exitButtonAnimation,
        LoadingBaySkyReadout skyReadout)
        : this()
    {
        ProductStateStore<LoadingBaySnapshot>? store = null;
        try
        {
            store = new ProductStateStore<LoadingBaySnapshot>(engine, "loading-bay", new LoadingBaySnapshotCodec());
            _engineContext = engine; _productContent = content; _exitPresentation = exitPresentation; _exitButtonAnimation = exitButtonAnimation; _skyReadout = skyReadout;
            _engineServices = new LoadingBayEngineServices(engine, content, _tuning, _entities, _player, exitPresentation, exitButtonAnimation, skyReadout);
            _playerSnapshot = _engineServices.CapturePlayer();
            _store = store;
        }
        catch
        {
            _engineServices?.Dispose();
            store?.Dispose();
            throw;
        }
    }

    /// <summary>Focused persistence seam: it composes the public Engine store without creating presentation services.</summary>
    internal LoadingBaySession(IPersistenceService persistence)
        : this()
    {
        _store = new ProductStateStore<LoadingBaySnapshot>(new PersistenceOnlyContext(persistence), "loading-bay", new LoadingBaySnapshotCodec());
    }

    public ProductUpdateResult Update(ProductUpdate update)
    {
        ThrowIfDisposed();
        _facts = update.Facts;
        _hasFacts = true;
        _scheduler.Advance(update);
        for (uint offset = 0; offset < update.Facts.AdmittedStepCount; offset++)
            _world.Advance(LoadingBayAdmittedStepTicks.At(update.Facts, offset), Record);
        _engineServices?.Update(update, _tuning, Record, CanCollectCanonicalPickup, CollectCanonicalPickup, ApplyCanonicalHazard, ActivateCanonicalFloor, ActivateCanonicalLift, DiscoverCanonicalSecret, ActivateCanonicalDoor, CompleteCanonicalExit, _world.Capture, PrepareWeaponFire, SettleWeaponFire, DamageCanonicalBarrel, EligibleEnemyEntities, PrepareEnemyAttacks, SettleEnemyAttack, RecordProjectileOutcome, ApplyProjectileDamage, ActivateEncounter);
        if (_engineServices is not null) _playerSnapshot = _engineServices.CapturePlayer();
        Publish();
        return ProductUpdateResult.None;
    }

    public void Publish()
    {
        ThrowIfDisposed();
        _engineServices?.Publish(Readout());
    }

    public void Attach()
    {
        ThrowIfDisposed();
        if (_engineServices is null)
        {
            Publish();
            return;
        }
        _engineServices.Attach(Readout());
    }

    public void ActivateSharedRealizations()
    {
        ThrowIfDisposed();
        _engineServices?.ActivateSharedRealizations();
        _sharedRealizationsActive = true;
    }

    public void DeactivateSharedRealizations()
    {
        ThrowIfDisposed();
        _engineServices?.DeactivateSharedRealizations();
        _sharedRealizationsActive = false;
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        _debugEntityWorldChanged = null;
        _journal.Clear();
        List<Exception>? failures = null;
        try { _engineServices?.Dispose(); }
        catch (Exception failure) { (failures ??= []).Add(failure); }
        try { _store?.Dispose(); }
        catch (Exception failure) { (failures ??= []).Add(failure); }
        try { _entities.Dispose(); }
        catch (Exception failure) { (failures ??= []).Add(failure); }
        foreach (Exception failure in _retiredProjectionFailures) (failures ??= []).Add(failure);
        if (_droppedRetiredProjectionFailures != 0)
            (failures ??= []).Add(new InvalidOperationException($"Loading Bay dropped {_droppedRetiredProjectionFailures} retired Engine-projection disposal failures."));
        if (failures is { Count: > 0 }) throw new AggregateException(failures);
    }

    internal LoadingBayReadout Readout() => new(_player, _hasFacts ? _facts : default, _health.Current.Raw, _armor.Current.Raw, _armorProtection, BulletQuantity(), ShellQuantity(), OwnedWeaponIds(), EquippedWeaponId(), WeaponCooldowns(), _playerSnapshot, PickupSnapshots(), ActorReadouts(), _complete, _scheduler.Readout.Pending, _tuning, _journal.ToArray(), _dropped);

    LoadingBayReadout ILoadingBaySession.Readout() => Readout();

    LoadingBayEngineServiceReadout ILoadingBaySession.EngineReadout()
        => _engineServices?.Readout ?? LoadingBayEngineServiceReadout.Empty;

    EntityWorld ILoadingBayDebugSession.DebugEntityWorld => _engineServices?.EntityWorld ?? _entities;

    void ILoadingBayDebugSession.SetDebugEntityWorldChanged(Action<EntityWorld>? callback)
        => _debugEntityWorldChanged = callback;

    internal LoadingBayReceipt CollectPickup(string pickup, LoadingBayItem item, ulong quantity)
    {
        ThrowIfDisposed();
        if (_manualPickupKeys.Contains(pickup)) return Reject("pickup.already-collected");
        if (_health.Current.Raw == 0) { _manualPickupKeys.Remove(pickup); return Reject("pickup.player-defeated"); }
        try
        {
            if (!CanApplyPickup(item)) return Reject("pickup.not-needed");
            if (item.PickupPolicy is LoadingBayPickupPolicy.Restore(var amount, var maximum, _))
            {
                long boundedMaximum = Math.Min(_tuning.MaximumHealth, maximum);
                _health.Restore(new Mechanics.ExactValue(amount), new Mechanics.ExactTrackBounds(new Mechanics.ExactValue(0), new Mechanics.ExactValue(boundedMaximum)));
            }
            else if (item.PickupPolicy is LoadingBayPickupPolicy.SetMinimum(var minimum, var setProtection))
            {
                _armor.Set(new Mechanics.ExactValue(Math.Max(_armor.Current.Raw, minimum)), Mechanics.ExactTrackSetPolicy.RejectOutOfBounds);
                _armorProtection = setProtection;
            }
            else if (item.PickupPolicy is LoadingBayPickupPolicy.RestoreArmor(var armorAmount, var armorMaximum, _, var armorProtection))
            {
                long boundedMaximum = Math.Min(_tuning.MaximumArmor, armorMaximum);
                bool hadProtection = _armor.Current.Raw > 0 && _armorProtection.Mode != LoadingBayArmorProtectionMode.None;
                _armor.Restore(new Mechanics.ExactValue(armorAmount), new Mechanics.ExactTrackBounds(new Mechanics.ExactValue(0), new Mechanics.ExactValue(boundedMaximum)));
                // E1M1's bonus armor preserves an existing green/blue armor class.
                if (!hadProtection) _armorProtection = armorProtection;
            }
            else _inventory.Grant(_player, item.MechanicsDefinition, quantity);
            _manualPickupKeys.Add(pickup);
            Record(new PickupCollectedFact(pickup, item.Id, quantity));
            return Accept("pickup.collected");
        }
        catch (Mechanics.MechanicsException) { _manualPickupKeys.Remove(pickup); return Reject("pickup.inventory-rejected"); }
    }

    internal LoadingBayReceipt CollectCanonicalPickup(ulong entityId)
    {
        LoadingBayE1M1PickupPlacement pickup = LoadingBayE1M1SemanticCatalog.Pickup(entityId);
        if (_pickupStates[entityId].Lifecycle == LoadingBayPickupLifecycle.Collected) return Reject("pickup.already-collected");
        if (_pickupStates[entityId].Lifecycle == LoadingBayPickupLifecycle.Dormant) return Reject("pickup.dormant");
        LoadingBayReceipt outcome = pickup.ProgramId == "pickup/weapon-starter"
            ? CollectWeaponStarter(pickup)
            : CollectPickup(CanonicalPickupKey(entityId), LoadingBayDefinitions.Item(pickup.ItemId), pickup.Quantity);
        _manualPickupKeys.Remove(CanonicalPickupKey(entityId));
        UpdatePickupState(pickup, outcome.Accepted ? LoadingBayPickupLifecycle.Collected : _pickupStates[entityId].Lifecycle, outcome.Code, _hasFacts ? _facts.SimulationStep : 0, 0);
        return outcome;
    }

    internal bool CanCollectCanonicalPickup(ulong entityId)
    {
        LoadingBayE1M1PickupPlacement pickup = LoadingBayE1M1SemanticCatalog.Pickup(entityId);
        if (_pickupStates[entityId].Lifecycle != LoadingBayPickupLifecycle.Active || _health.Current.Raw == 0) return false;
        return pickup.ProgramId == "pickup/weapon-starter"
            ? CanApplyWeaponStarter(pickup)
            : CanApplyPickup(LoadingBayDefinitions.Item(pickup.ItemId));
    }

    internal LoadingBayReceipt ApplyDamage(string target, int damage, string cause)
    {
        ThrowIfDisposed();
        if (target != "player") return Reject("damage.unknown-target");
        if (damage <= 0) return Reject("damage.invalid");
        if (_health.Current.Raw == 0) return Reject("damage.target-defeated");
        long absorbed = _armorProtection.AbsorptionDivisor == 0 ? 0 : Math.Min(_armor.Current.Raw, damage / _armorProtection.AbsorptionDivisor);
        if (absorbed > 0) _armor.Spend(new Mechanics.ExactValue(absorbed));
        long applied = Math.Min(_health.Current.Raw, damage - absorbed);
        if (applied > 0) _health.Spend(new Mechanics.ExactValue(applied));
        bool defeated = _health.Current.Raw == 0;
        Record(new DamageAppliedFact(target, damage, absorbed, applied, cause, defeated));
        return Accept(defeated ? "damage.defeated" : "damage.applied");
    }

    internal LoadingBayReceipt ActivateEncounter(ulong encounterEntityId, ulong tick)
    {
        LoadingBayE1M1EncounterDefinition encounter = LoadingBayE1M1SemanticCatalog.Encounters.Single(value => value.EntityId == encounterEntityId);
        if (!_activatedEncounters.Add(encounter.EntityId)) return Reject("encounter.already-active");
        Record(new EncounterActivatedFact(encounter.EntityId, encounter.Label, encounter.ActivationRadius, tick));
        foreach (ulong member in encounter.Members)
        {
            LoadingBayE1M1EnemyDefinition enemy = LoadingBayE1M1SemanticCatalog.Enemy(member);
            EnemyState state = _actors[member];
            if (state.Posture == LoadingBayEnemyPosture.Dormant)
            {
                state.Posture = LoadingBayEnemyPosture.Active;
                Record(new EnemyPostureChangedFact(member, state.Posture, state.Health, tick, "encounter.activated"));
            }
        }
        return Accept("encounter.activated");
    }

    internal LoadingBayReceipt ApplyWeaponDamage(ulong enemyEntityId, string weaponId, int damage, ulong tick)
    {
        ThrowIfDisposed();
        if (damage <= 0 || !LoadingBayDefinitions.Weapons.ContainsKey(weaponId)) return Reject("combat.invalid-hit");
        if (!_actors.TryGetValue(enemyEntityId, out EnemyState? state)) return Reject("combat.unknown-enemy");
        if (state.Posture is LoadingBayEnemyPosture.Dormant or LoadingBayEnemyPosture.Defeated) return Reject("combat.ineligible-target");
        state.Health = Math.Max(0, state.Health - damage);
        Record(new EnemyHitFact(enemyEntityId, weaponId, damage, state.Health, tick));
        if (state.Health > 0)
        {
            LoadingBayE1M1EnemyDefinition enemy = LoadingBayE1M1SemanticCatalog.Enemy(enemyEntityId);
            state.Posture = LoadingBayEnemyPosture.Pained;
            state.ReadyAtTick = checked(tick + (ulong)enemy.PainDurationTicks);
            Record(new EnemyPostureChangedFact(enemyEntityId, state.Posture, state.Health, tick, "combat.pain"));
            return Accept("combat.hit");
        }
        state.Posture = LoadingBayEnemyPosture.Defeated;
        state.Visible = false;
        LoadingBayE1M1EnemyDefinition defeated = LoadingBayE1M1SemanticCatalog.Enemy(enemyEntityId);
        Record(new EnemyPostureChangedFact(enemyEntityId, state.Posture, 0, tick, "combat.defeated"));
        Record(new EnemyDefeatedFact(enemyEntityId, defeated.DropPickupEntityId, tick));
        if (defeated.DropPickupEntityId != 0)
        {
            LoadingBayE1M1PickupPlacement drop = LoadingBayE1M1SemanticCatalog.Pickup(defeated.DropPickupEntityId);
            UpdatePickupState(drop, LoadingBayPickupLifecycle.Active, "enemy.drop-materialized", tick, 0);
            if (_engineServices is not null) Record(_engineServices.MaterializeEnemyDrop(defeated.DropPickupEntityId, defeated.Translation, tick));
        }
        foreach (LoadingBayE1M1EncounterDefinition encounter in LoadingBayE1M1SemanticCatalog.Encounters.Where(encounter => _activatedEncounters.Contains(encounter.EntityId) && encounter.Members.Contains(enemyEntityId)))
        {
            if (encounter.Members.All(member => _actors[member].Posture == LoadingBayEnemyPosture.Defeated))
                Record(new EncounterChangedFact(encounter.Label, true));
        }
        return Accept("combat.defeated");
    }

    internal LoadingBayWeaponFirePlan? PrepareWeaponFire(ulong tick)
    {
        ThrowIfDisposed();
        string? weaponId = EquippedWeaponId();
        if (weaponId is null) { Reject("combat.no-equipped-weapon"); return null; }
        LoadingBayE1M1Weapon weapon = LoadingBayE1M1SemanticCatalog.Item<LoadingBayE1M1Weapon>(weaponId);
        if (_weaponReadyAt.TryGetValue(weaponId, out ulong readyAt) && tick < readyAt) { Reject("combat.weapon-cooldown"); return null; }
        if (weapon.AmmunitionCost > 0 && ItemQuantity(LoadingBayDefinitions.Item(weapon.AmmunitionId)) < (ulong)weapon.AmmunitionCost) { Reject("combat.insufficient-ammunition"); return null; }
        return new LoadingBayWeaponFirePlan(weaponId, weapon.AmmunitionId, weapon.AmmunitionCost, weapon.DamageRolls, weapon.Damage, weapon.PelletCount == 0 ? 1 : weapon.PelletCount, weapon.SpreadDegrees, weapon.MaximumDistance, tick);
    }

    internal IReadOnlySet<ulong> EligibleEnemyEntities() => _actors
        .Where(pair => pair.Value.Posture is LoadingBayEnemyPosture.Active or LoadingBayEnemyPosture.Pained)
        .Select(pair => pair.Key).ToHashSet();

    internal LoadingBayReceipt SettleWeaponFire(LoadingBayWeaponFirePlan plan, IReadOnlyList<LoadingBayWeaponImpact> impacts)
    {
        ThrowIfDisposed();
        if (plan.WeaponId != EquippedWeaponId()) return Reject("combat.stale-fire-plan");
        LoadingBayE1M1Weapon weapon = LoadingBayE1M1SemanticCatalog.Item<LoadingBayE1M1Weapon>(plan.WeaponId);
        if (_weaponReadyAt.TryGetValue(plan.WeaponId, out ulong readyAt) && plan.Tick < readyAt) return Reject("combat.weapon-cooldown");
        try
        {
            if (plan.AmmunitionCost > 0) _inventory.Consume(_player, LoadingBayDefinitions.Item(plan.AmmunitionId).MechanicsDefinition, (ulong)plan.AmmunitionCost);
        }
        catch (Mechanics.MechanicsException) { return Reject("combat.insufficient-ammunition"); }
        _weaponReadyAt[plan.WeaponId] = checked(plan.Tick + (ulong)weapon.CooldownTicks);
        Record(new WeaponFiredFact(plan.WeaponId, plan.Tick, plan.PelletCount, impacts.Count));
        foreach (LoadingBayWeaponImpact impact in impacts)
        {
            if (impact.EnemyEntityId == 0) Record(new WeaponMissedFact(plan.WeaponId, plan.Tick, impact.PelletIndex, impact.WorldOccluded ? "combat.world-occluded" : "combat.miss"));
            else _ = ApplyWeaponDamage(impact.EnemyEntityId, plan.WeaponId, impact.Damage, plan.Tick);
        }
        return Accept("combat.fired");
    }

    /// <summary>Product policy consumes Engine visibility evidence and admits only active, ready actors.</summary>
    internal IReadOnlyList<LoadingBayEnemyAttackPlan> PrepareEnemyAttacks(ulong tick, IReadOnlySet<ulong> visibleEnemies, uint visibilityCasts, uint occlusionRejects)
    {
        ThrowIfDisposed();
        List<LoadingBayEnemyAttackPlan> plans = [];
        foreach (LoadingBayE1M1EnemyDefinition enemy in LoadingBayE1M1SemanticCatalog.Enemies)
        {
            EnemyState state = _actors[enemy.EntityId];
            if (state.Posture == LoadingBayEnemyPosture.Pained && tick >= state.ReadyAtTick)
            {
                state.Posture = LoadingBayEnemyPosture.Active;
                Record(new EnemyPostureChangedFact(enemy.EntityId, state.Posture, state.Health, tick, "combat.pain-recovered"));
            }
            bool visible = visibleEnemies.Contains(enemy.EntityId);
            if ((state.Posture is LoadingBayEnemyPosture.Active or LoadingBayEnemyPosture.Pained) && state.Visible != visible)
            {
                state.Visible = visible;
                Record(new EnemyPerceptionFact(enemy.EntityId, visible, tick, visibilityCasts, occlusionRejects));
            }
            if (!visible || state.Posture != LoadingBayEnemyPosture.Active || tick < state.ReadyAtTick) continue;
            plans.Add(new LoadingBayEnemyAttackPlan(
                enemy.EntityId, enemy.AttackKind, enemy.Translation + enemy.AttackOriginOffset,
                enemy.AttackDamage, enemy.AttackRange, enemy.AttackCooldownTicks,
                (float)enemy.ProjectileMass, (float)enemy.ProjectileRadius, (float)enemy.ProjectileImpulse,
                (float)enemy.ProjectileGravityScale, enemy.ProjectileLifetimeTicks, (float)enemy.ProjectileRestitution, tick));
        }
        return plans;
    }

    /// <summary>Only a completed Engine combat execution advances the corresponding actor's readiness.</summary>
    internal LoadingBayReceipt SettleEnemyAttack(LoadingBayEnemyAttackPlan plan, bool hitPlayer, string cause)
    {
        ThrowIfDisposed();
        if (!_actors.TryGetValue(plan.EnemyEntityId, out EnemyState? state) || state.Posture != LoadingBayEnemyPosture.Active || state.ReadyAtTick > plan.Tick)
            return Reject("combat.stale-enemy-attack");
        LoadingBayE1M1EnemyDefinition enemy = LoadingBayE1M1SemanticCatalog.Enemy(plan.EnemyEntityId);
        state.ReadyAtTick = checked(plan.Tick + (ulong)enemy.AttackCooldownTicks);
        Record(new EnemyAttackFact(plan.EnemyEntityId, plan.Kind, hitPlayer, plan.Tick, cause));
        if (plan.Kind == LoadingBayE1M1EnemyAttackKind.Projectile)
            Record(new EnemyProjectileFact(plan.EnemyEntityId, plan.Tick, "combat.projectile-realized"));
        return hitPlayer ? ApplyDamage("player", plan.Damage, $"enemy.{enemy.Label}") : Accept(cause);
    }

    internal void RecordProjectileOutcome(ulong enemyEntityId, ulong tick, string cause) => Record(new EnemyProjectileFact(enemyEntityId, tick, cause));

    internal LoadingBayReceipt ApplyProjectileDamage(ulong enemyEntityId, int damage, ulong tick)
    {
        ThrowIfDisposed();
        if (!_actors.TryGetValue(enemyEntityId, out EnemyState? state) || state.Posture == LoadingBayEnemyPosture.Defeated || damage <= 0)
            return Reject("combat.invalid-projectile-impact");
        LoadingBayE1M1EnemyDefinition enemy = LoadingBayE1M1SemanticCatalog.Enemy(enemyEntityId);
        Record(new EnemyAttackFact(enemyEntityId, LoadingBayE1M1EnemyAttackKind.Projectile, true, tick, "combat.projectile-player-impact"));
        return ApplyDamage("player", damage, $"enemy.{enemy.Label}.projectile");
    }

    internal LoadingBayReceipt SetDoor(string door, bool open, ulong closeAfterSteps = 0)
    {
        ThrowIfDisposed();
        if (open && closeAfterSteps > 0) return Reject("door.timing-deferred");
        _doors[door] = open;
        Record(new DoorChangedFact(door, open));
        return Accept(open ? "door.opened" : "door.closed");
    }

    internal LoadingBayReceipt DiscoverSecret(string secret)
    {
        ThrowIfDisposed();
        if (!_secrets.Add(secret)) return Reject("secret.already-discovered");
        Record(new SecretDiscoveredFact(secret)); return Accept("secret.discovered");
    }

    internal LoadingBayReceipt CompleteExit(string exit)
    {
        ThrowIfDisposed();
        if (_health.Current.Raw == 0) return Reject("exit.player-defeated");
        if (_complete) return Reject("exit.already-complete");
        _complete = true; Record(new ExitCompletedFact(exit)); return Accept("exit.completed");
    }

    /// <summary>Called only after Engine's overlap coordinator has established a canonical hazard enter/stay fact.</summary>
    internal LoadingBayReceipt ApplyCanonicalHazard(ulong hazardEntityId, ulong tick)
    {
        if (!_world.HazardReady(hazardEntityId, tick)) return Reject("hazard.cooldown");
        try { _world.ApplyHazard(hazardEntityId, tick, (damage, cause) => ApplyDamage("player", damage, cause), Record); return Accept("hazard.applied"); }
        catch (InvalidOperationException) { return Reject("hazard.unknown"); }
    }

    internal LoadingBayReceipt ActivateCanonicalDoor(ulong doorEntityId, ulong tick)
    {
        try
        {
            if (_world.Capture().Doors.Single(value => value.EntityId == doorEntityId).State is not (LoadingBayDoorState.Closed or LoadingBayDoorState.Closing)) return Reject("door.unavailable");
            LoadingBayDoorSnapshot state = _world.ActivateDoor(doorEntityId, tick, Record); if (state.DueStep > tick) ScheduleWorldContinuation(state.DueStep); return Accept("door.opening");
        }
        catch (InvalidOperationException) { return Reject("door.unknown"); }
    }

    internal LoadingBayReceipt ActivateCanonicalFloor(ulong floorEntityId, ulong tick)
    {
        try
        {
            if (_world.Capture().Floors.Single(value => value.EntityId == floorEntityId).State != LoadingBayFloorState.Armed) return Reject("floor.unavailable");
            LoadingBayFloorSnapshot state = _world.ActivateFloor(floorEntityId, tick, Record); if (state.DueStep > tick) ScheduleWorldContinuation(state.DueStep); return Accept("floor.lowering");
        }
        catch (InvalidOperationException) { return Reject("floor.unknown"); }
    }

    internal LoadingBayReceipt ActivateCanonicalLift(ulong liftEntityId, ulong tick)
    {
        try
        {
            if (_world.Capture().Lifts.Single(value => value.EntityId == liftEntityId).State != LoadingBayLiftState.Raised) return Reject("lift.unavailable");
            LoadingBayLiftSnapshot state = _world.ActivateLift(liftEntityId, tick, Record); if (state.DueStep > tick) ScheduleWorldContinuation(state.DueStep); return Accept("lift.lowering");
        }
        catch (InvalidOperationException) { return Reject("lift.unknown"); }
    }

    internal LoadingBayReceipt DamageCanonicalBarrel(ulong barrelEntityId, int damage, ulong tick)
    {
        try
        {
            Func<LoadingBayE1M1BarrelDefinition, LoadingBayE1M1BarrelDefinition, bool> occluded = _engineServices is null
                ? static (_, _) => false
                : _engineServices.BarrelOccluded;
            foreach (LoadingBayE1M1BarrelDefinition barrel in _world.DamageBarrel(barrelEntityId, damage, tick, occluded, Record))
                RecordWorldAction("barrel.exploded", barrel.Label);
            return Accept("barrel.damage-applied");
        }
        catch (InvalidOperationException) { return Reject("barrel.unknown"); }
    }

    internal LoadingBayReceipt DiscoverCanonicalSecret(ulong secretEntityId)
    {
        try { return DiscoverSecret(LoadingBayE1M1SemanticCatalog.Secrets.Single(value => value.EntityId == secretEntityId).Label); }
        catch (InvalidOperationException) { return Reject("secret.unknown"); }
    }

    internal LoadingBayReceipt CompleteCanonicalExit(ulong exitEntityId)
    {
        try { return CompleteExit(LoadingBayE1M1SemanticCatalog.Exits.Single(value => value.EntityId == exitEntityId).Label); }
        catch (InvalidOperationException) { return Reject("exit.unknown"); }
    }

    /// <summary>Consumes already-resolved world facts; it deliberately performs no spatial query or presentation work.</summary>
    internal LoadingBayReceipt ResolveWorldAction(LoadingBayWorldAction action)
    {
        ThrowIfDisposed();
        return action switch
        {
            LoadingBayWorldAction.EncounterActivated(var encounter, _) => ActivateCanonicalEncounterByLabel(encounter),
            LoadingBayWorldAction.EnemyDefeated(var enemy) => Reject($"world-action.enemy-defeat-deferred:{enemy}"),
            LoadingBayWorldAction.HazardApplied(var damage, var cause) => ApplyDamage("player", damage, cause),
            LoadingBayWorldAction.BarrelExploded(var damage) => ApplyDamage("player", damage, "barrel"),
            LoadingBayWorldAction.FloorActivated(var floor) => RecordWorldAction("floor.activated", floor),
            LoadingBayWorldAction.LiftActivated(var lift) => RecordWorldAction("lift.activated", lift),
            LoadingBayWorldAction.SwitchActivated(var door) => SetDoor(door, true),
            _ => throw new ArgumentOutOfRangeException(nameof(action)),
        };
    }

    internal LoadingBayReceipt DeveloperSetTrack(ulong generation, string track, int value, string correlation)
    {
        ThrowIfDisposed();
        ulong currentGeneration = _hasFacts ? _facts.Generation : 0;
        if (generation != currentGeneration) return Reject("developer.stale-generation", correlation);
        Mechanics.ExactTrack selected = track switch { "health" => _health, "armor" => _armor, _ => throw new ArgumentOutOfRangeException(nameof(track)) };
        try { selected.Set(new Mechanics.ExactValue(value), Mechanics.ExactTrackSetPolicy.RejectOutOfBounds); }
        catch (Mechanics.MechanicsException) { return Reject("developer.track-rejected", correlation); }
        Record(new DeveloperTrackChangedFact(track, value, correlation)); return Accept("developer.track-set", correlation);
    }

    LoadingBayReceipt ILoadingBaySession.DeveloperSetTrack(ulong generation, string track, int value, string correlation)
        => DeveloperSetTrack(generation, track, value, correlation);

    internal LoadingBaySnapshot Capture(string contentIdentity) => new(contentIdentity, _health.Current.Raw, _armor.Current.Raw, _armorProtection, BulletQuantity(), ShellQuantity(), OwnedWeaponIds(), EquippedWeaponId(), WeaponCooldowns(), _engineServices?.CapturePlayer() ?? _playerSnapshot, PickupSnapshots(), _secrets.OrderBy(x => x, StringComparer.Ordinal).ToArray(), _complete, _doors.OrderBy(x => x.Key, StringComparer.Ordinal).Select(x => new LoadingBayNamedState(x.Key, x.Value)).ToArray(), ActorSnapshots(), EncounterSnapshots(), _world.Capture());

    internal LoadingBayReceipt Restore(LoadingBaySnapshot snapshot, string identity)
    {
        ThrowIfDisposed();
        if (snapshot.ContentIdentity != identity) return Reject("save.content-identity-mismatch");
        if (snapshot.Health < 0 || snapshot.Health > _tuning.MaximumHealth || snapshot.Armor < 0 || snapshot.Armor > _tuning.MaximumArmor || !LoadingBayDefinitions.IsKnownArmorProtection(snapshot.ArmorProtection)) return Reject("save.invalid-track");
        if (snapshot.Bullets > LoadingBayDefinitions.Bullets.MechanicsDefinition.MaximumQuantity || snapshot.Shells > LoadingBayDefinitions.Shells.MechanicsDefinition.MaximumQuantity) return Reject("save.invalid-inventory");
        LoadingBayWorldSnapshot worldBeforeValidation = _world.Capture();
        bool validWorld = _world.TryRestore(snapshot.World);
        _world.TryRestore(worldBeforeValidation);
        if (!ValidPickupSet(snapshot.Pickups) || !ValidPlayer(snapshot.Player) || !ValidCooldowns(snapshot.WeaponCooldowns, snapshot.OwnedWeapons) || !ValidDistinct(snapshot.Secrets) || !ValidState(snapshot.Doors) || !ValidActors(snapshot.Actors) || !ValidWeapons(snapshot.OwnedWeapons, snapshot.EquippedWeapon) || !ValidEncounters(snapshot.Encounters, snapshot.Actors) || !validWorld) return Reject("save.invalid-collection");
        LoadingBaySnapshot previous = Capture(identity);
        IReadOnlyList<CanonicalPickupTriggerStateFact>? triggerFacts = null;
        try { ApplySnapshot(snapshot); }
        catch (Exception applicationFailure)
        {
            List<Exception> failures = [applicationFailure];
            try { ApplySnapshot(previous); }
            catch (Exception rollbackFailure) { failures.Add(rollbackFailure); }
            if (failures.Count > 1) throw new AggregateException(failures);
            if (applicationFailure is Mechanics.MechanicsException) return Reject("save.invalid-equipment");
            throw;
        }
        try
        {
            if (_engineServices is not null)
            {
                // Engine continuations restore only into a fresh compatible Spatial session.
                LoadingBayEngineServices replacement = CreateFreshEngineServices();
                try
                {
                    replacement.RestorePlayer(snapshot.Player);
                    triggerFacts = replacement.RestoreSemanticPickups(snapshot.Pickups);
                    replacement.RestoreEncounterActivations(_activatedEncounters);
                    replacement.RestoreWorldMotion(snapshot.World, _hasFacts ? _facts.SimulationStep : 0);
                    if (_sharedRealizationsActive) replacement.ActivateSharedRealizations();
                }
                catch
                {
                    replacement.Dispose();
                    throw;
                }
                LoadingBayEngineServices previousServices = _engineServices;
                _engineServices = replacement;
                try
                {
                    _debugEntityWorldChanged?.Invoke(replacement.EntityWorld);
                }
                catch (Exception debugWorldFailure)
                {
                    _engineServices = previousServices;
                    try { replacement.Dispose(); }
                    catch (Exception replacementCleanupFailure) { throw new AggregateException(debugWorldFailure, replacementCleanupFailure); }
                    throw;
                }
                RetireProjection(previousServices);
            }
        }
        catch
        {
            // Product state and derived Engine motion must return together; no partial save restore is observable.
            ApplySnapshot(previous);
            return Reject("save.world-motion-restore-rejected");
        }
        if (triggerFacts is not null) foreach (CanonicalPickupTriggerStateFact triggerFact in triggerFacts) Record(triggerFact);
        Record(new SnapshotRestoredFact(identity)); return Accept("save.restored");
    }

    internal LoadingBayReceipt Save(string slot)
    {
        ThrowIfDisposed();
        if (_store is null) return Reject("save.persistence-unavailable");
        _store.Save(slot, Capture(_tuning.ContentIdentity));
        return Accept("save.written");
    }

    internal LoadingBayReceipt Load(string slot)
    {
        ThrowIfDisposed();
        if (_store is null) return Reject("save.persistence-unavailable");
        ProductStateLoad<LoadingBaySnapshot> loaded = _store.Load(slot);
        return !loaded.Present || loaded.State is null ? Reject("save.empty") : Restore(loaded.State, _tuning.ContentIdentity);
    }

    private void Record(LoadingBayFact fact)
    {
        if (fact is PickupLifecycleFact lifecycle)
        {
            LoadingBayE1M1PickupPlacement pickup = LoadingBayE1M1SemanticCatalog.Pickup(lifecycle.PickupEntityId);
            _pickupStates[lifecycle.PickupEntityId] = new LoadingBayPickupSnapshot(lifecycle.PickupEntityId, pickup.ItemId, pickup.ProgramId, lifecycle.Lifecycle, lifecycle.Cause, lifecycle.Tick, lifecycle.TriggerRevision);
        }
        if (_journal.Count == _tuning.FactJournalCapacity) { _journal.Dequeue(); _dropped++; }
        _journal.Enqueue(fact);
    }
    private static string CanonicalPickupKey(ulong entityId) => $"e1m1.pickup.{entityId}";
    private void UpdatePickupState(LoadingBayE1M1PickupPlacement pickup, LoadingBayPickupLifecycle lifecycle, string cause, ulong tick, ulong triggerRevision)
    {
        _pickupStates[pickup.EntityId] = new LoadingBayPickupSnapshot(pickup.EntityId, pickup.ItemId, pickup.ProgramId, lifecycle, cause, tick, triggerRevision);
    }
    private void ApplySnapshot(LoadingBaySnapshot snapshot)
    {
        RestoreWeapons(snapshot.OwnedWeapons, snapshot.EquippedWeapon);
        _health.Set(new Mechanics.ExactValue(snapshot.Health), Mechanics.ExactTrackSetPolicy.RejectOutOfBounds);
        _armor.Set(new Mechanics.ExactValue(snapshot.Armor), Mechanics.ExactTrackSetPolicy.RejectOutOfBounds);
        _armorProtection = snapshot.ArmorProtection;
        SetBulletQuantity(snapshot.Bullets);
        SetShellQuantity(snapshot.Shells);
        _pickupStates.Clear(); foreach (LoadingBayPickupSnapshot pickup in snapshot.Pickups) _pickupStates.Add(pickup.EntityId, pickup);
        _weaponReadyAt.Clear(); foreach (LoadingBayWeaponCooldownSnapshot cooldown in snapshot.WeaponCooldowns) _weaponReadyAt.Add(cooldown.WeaponId, cooldown.ReadyAtTick);
        _playerSnapshot = snapshot.Player;
        _secrets.Clear(); foreach (string id in snapshot.Secrets) _secrets.Add(id);
        _doors.Clear(); foreach (LoadingBayNamedState door in snapshot.Doors) _doors.Add(door.Id, door.Value);
        foreach (LoadingBayActorSnapshot actor in snapshot.Actors)
        {
            EnemyState state = _actors[actor.EntityId];
            state.Health = actor.Health;
            state.Posture = actor.Posture;
            state.Visible = actor.Visible;
            state.ReadyAtTick = actor.ReadyAtTick;
        }
        _activatedEncounters.Clear();
        foreach (LoadingBayEncounterSnapshot encounter in snapshot.Encounters.Where(encounter => encounter.Activated)) _activatedEncounters.Add(encounter.EntityId);
        _complete = snapshot.Complete;
        if (!_world.TryRestore(snapshot.World)) throw new InvalidOperationException("A prevalidated world snapshot could not be restored.");
        _scheduler = new SimulationScheduler();
        foreach (ulong dueStep in _world.DueSteps()) ScheduleWorldContinuation(dueStep);
    }
    private LoadingBayReceipt RecordWorldAction(string code, string subject) { Record(new WorldActionFact(code, subject)); return Accept(code); }
    private void ScheduleWorldContinuation(ulong dueStep) => _scheduler.ScheduleAt(dueStep, context => _world.Advance(context.SimulationStep, Record));
    private LoadingBayEngineServices CreateFreshEngineServices()
    {
        if (_engineContext is null || _productContent is null || _exitPresentation is null || _exitButtonAnimation is null)
            throw new InvalidOperationException("Loading Bay cannot rebuild an Engine-backed continuation without its product composition inputs.");
        EntityWorld projection = new([
            EngineComponentTypes.Transform,
            EngineComponentTypes.SpatialCollider,
            EngineComponentTypes.Kinematic,
        ]);
        try
        {
            BootstrapCanonicalEntities(projection);
            return new LoadingBayEngineServices(_engineContext, _productContent, _tuning, projection, _player, _exitPresentation, _exitButtonAnimation, _skyReadout, ownsProjectionEntities: true);
        }
        catch
        {
            projection.Dispose();
            throw;
        }
    }
    private void RetireProjection(LoadingBayEngineServices projection)
    {
        try { projection.Dispose(); }
        catch (Exception failure)
        {
            const int MaximumRetirementFailures = 8;
            if (_retiredProjectionFailures.Count == MaximumRetirementFailures) { _retiredProjectionFailures.Dequeue(); _droppedRetiredProjectionFailures++; }
            _retiredProjectionFailures.Enqueue(failure);
            Record(new RejectedFact("save.previous-projection-dispose-failed", null));
        }
    }
    private LoadingBayReceipt ActivateCanonicalEncounterByLabel(string label)
    {
        LoadingBayE1M1EncounterDefinition? encounter = LoadingBayE1M1SemanticCatalog.Encounters.SingleOrDefault(value => value.Label == label);
        return encounter is null ? Reject("encounter.unknown") : ActivateEncounter(encounter.EntityId, _hasFacts ? _facts.SimulationStep : 0);
    }
    private LoadingBayReceipt Accept(string code, string? correlation = null) => new(true, code, correlation);
    private LoadingBayReceipt Reject(string code, string? correlation = null) { Record(new RejectedFact(code, correlation)); return new(false, code, correlation); }
    private ulong BulletQuantity() => _inventory.Read(_player).Stacks.SingleOrDefault(stack => stack.Definition == LoadingBayDefinitions.Bullets.MechanicsDefinition.Id).Quantity;
    private LoadingBayPickupSnapshot[] PickupSnapshots() => _pickupStates.Values.OrderBy(state => state.EntityId).ToArray();
    private LoadingBayWeaponCooldownSnapshot[] WeaponCooldowns() => _weaponReadyAt.OrderBy(pair => pair.Key, StringComparer.Ordinal).Select(pair => new LoadingBayWeaponCooldownSnapshot(pair.Key, pair.Value)).ToArray();
    private LoadingBayEnemyReadout[] ActorReadouts() => LoadingBayE1M1SemanticCatalog.Enemies.Select(enemy =>
    {
        EnemyState state = _actors[enemy.EntityId];
        return new LoadingBayEnemyReadout(enemy.EntityId, enemy.Label, state.Health, state.Posture, state.Visible, state.ReadyAtTick, enemy.DropPickupEntityId);
    }).ToArray();
    private LoadingBayActorSnapshot[] ActorSnapshots() => LoadingBayE1M1SemanticCatalog.Enemies.Select(enemy =>
    {
        EnemyState state = _actors[enemy.EntityId];
        return new LoadingBayActorSnapshot(enemy.EntityId, state.Health, state.Posture, state.Visible, state.ReadyAtTick);
    }).ToArray();
    private LoadingBayEncounterSnapshot[] EncounterSnapshots() => LoadingBayE1M1SemanticCatalog.Encounters
        .Select(encounter => new LoadingBayEncounterSnapshot(encounter.EntityId, _activatedEncounters.Contains(encounter.EntityId), encounter.Members.All(member => _actors[member].Posture == LoadingBayEnemyPosture.Defeated)))
        .ToArray();
    private ulong ShellQuantity() => _inventory.Read(_player).Stacks.SingleOrDefault(stack => stack.Definition == LoadingBayDefinitions.Shells.MechanicsDefinition.Id).Quantity;
    private ulong ItemQuantity(LoadingBayItem item) => _inventory.Read(_player).Stacks.SingleOrDefault(stack => stack.Definition == item.MechanicsDefinition.Id).Quantity;
    private void SetBulletQuantity(ulong quantity)
    {
        ulong current = BulletQuantity();
        if (current < quantity) _inventory.Grant(_player, LoadingBayDefinitions.Bullets.MechanicsDefinition, quantity - current);
        else if (current > quantity) _inventory.Consume(_player, LoadingBayDefinitions.Bullets.MechanicsDefinition, current - quantity);
    }
    private void SetShellQuantity(ulong quantity)
    {
        ulong current = ShellQuantity();
        if (current < quantity) _inventory.Grant(_player, LoadingBayDefinitions.Shells.MechanicsDefinition, quantity - current);
        else if (current > quantity) _inventory.Consume(_player, LoadingBayDefinitions.Shells.MechanicsDefinition, current - quantity);
    }
    private void ApplyPlayerSetup(LoadingBayE1M1PlayerSetup setup)
    {
        foreach (LoadingBayE1M1ItemGrant grant in setup.Grants)
        {
            if (LoadingBayDefinitions.Weapons.TryGetValue(grant.ItemId, out LoadingBayWeapon? weapon)) MaterializeWeapon(weapon);
            else _inventory.Grant(_player, LoadingBayDefinitions.Item(grant.ItemId).MechanicsDefinition, grant.Quantity);
        }
        EquipWeapon(setup.EquippedWeaponId);
    }
    private bool CanApplyWeaponStarter(LoadingBayE1M1PickupPlacement pickup)
    {
        if (pickup.StarterAmmunitionItemId is null || pickup.StarterAmmunitionQuantity == 0 || !LoadingBayDefinitions.Weapons.TryGetValue(pickup.ItemId, out LoadingBayWeapon? weapon)) return false;
        LoadingBayItem starterAmmo = LoadingBayDefinitions.Item(pickup.StarterAmmunitionItemId);
        ulong quantity = ItemQuantity(starterAmmo);
        if (quantity > starterAmmo.MechanicsDefinition.MaximumQuantity - pickup.StarterAmmunitionQuantity) return false;
        try
        {
            Mechanics.InventoryWorldCandidate candidate = _inventory.Prepare();
            if (!OwnedWeaponIds().Contains(weapon.Id, StringComparer.Ordinal))
                candidate.MaterializeUnique(new Mechanics.ItemState(new EntityId(_entities.NextEntityValue), weapon.MechanicsDefinition), _player);
            candidate.Grant(_player, starterAmmo.MechanicsDefinition, pickup.StarterAmmunitionQuantity);
            candidate.Validate();
            return true;
        }
        catch (Mechanics.MechanicsException) { return false; }
    }
    private LoadingBayReceipt CollectWeaponStarter(LoadingBayE1M1PickupPlacement pickup)
    {
        string key = CanonicalPickupKey(pickup.EntityId);
        if (_pickupStates[pickup.EntityId].Lifecycle == LoadingBayPickupLifecycle.Collected) return Reject("pickup.already-collected");
        if (_health.Current.Raw == 0) return Reject("pickup.player-defeated");
        if (!CanApplyWeaponStarter(pickup)) return Reject("pickup.not-needed");
        if (pickup.StarterAmmunitionItemId is null || !LoadingBayDefinitions.Weapons.TryGetValue(pickup.ItemId, out LoadingBayWeapon? weapon)) return Reject("pickup.invalid-catalog");
        EntityId? newWeaponEntity = null;
        try
        {
            Mechanics.InventoryWorldCandidate candidate = _inventory.Prepare();
            if (!OwnedWeaponIds().Contains(weapon.Id, StringComparer.Ordinal))
            {
                newWeaponEntity = _entities.Create();
                candidate.MaterializeUnique(new Mechanics.ItemState(newWeaponEntity.Value, weapon.MechanicsDefinition), _player);
            }
            candidate.Grant(_player, LoadingBayDefinitions.Item(pickup.StarterAmmunitionItemId).MechanicsDefinition, pickup.StarterAmmunitionQuantity);
            candidate.Publish();
            Record(new PickupCollectedFact(key, pickup.ItemId, pickup.Quantity));
            Record(new PickupLoadoutChangedFact(pickup.EntityId, pickup.ItemId, pickup.ProgramId, false, newWeaponEntity is null ? "pickup.weapon-ammunition" : "pickup.weapon-acquired"));
            return Accept("pickup.collected");
        }
        catch (Mechanics.MechanicsException)
        {
            if (newWeaponEntity is EntityId entity && _entities.IsAlive(entity)) _entities.Destroy(entity);
            return Reject("pickup.inventory-rejected");
        }
    }
    private static Mechanics.ExactTrack Track(string name, int current, int maximum) => new(new Mechanics.ExactTrackDefinition(Mechanics.TrackId.Parse($"loading-bay.{name}"), new Mechanics.ExactValue(0), new Mechanics.ExactTrackMaximum.Fixed(new Mechanics.ExactValue(maximum))), new Mechanics.ExactValue(current));
    private static LoadingBayPlayerSnapshot InitialPlayerSnapshot(LoadingBayTuning tuning) => new(
        tuning.InitialPosition,
        new LookState(-(tuning.InitialYawDegrees * (MathF.PI / 180f)), tuning.InitialPitchDegrees * (MathF.PI / 180f)),
        null);
    private static void BootstrapCanonicalEntities(EntityWorld entities)
    {
        for (ulong expected = 1; expected <= LoadingBayE1M1SemanticCatalog.CanonicalEntityCount; expected++)
        {
            EntityId entity = entities.Create();
            if (entity.Value != expected) throw new InvalidOperationException("Canonical E1M1 entity bootstrap drifted.");
        }
    }
    private static bool ValidDistinct(string[] values) => values.Length <= 256 && values.All(value => !string.IsNullOrWhiteSpace(value)) && values.Distinct(StringComparer.Ordinal).Count() == values.Length;
    private static bool ValidState(LoadingBayNamedState[] values) => values.Length <= 256 && values.All(value => !string.IsNullOrWhiteSpace(value.Id)) && values.Select(value => value.Id).Distinct(StringComparer.Ordinal).Count() == values.Length;
    private static bool ValidPickupSet(LoadingBayPickupSnapshot[] pickups)
    {
        if (pickups.Length != LoadingBayE1M1SemanticCatalog.Pickups.Length || pickups.Select(pickup => pickup.EntityId).Distinct().Count() != pickups.Length) return false;
        foreach (LoadingBayPickupSnapshot state in pickups)
        {
            LoadingBayE1M1PickupPlacement placement;
            try { placement = LoadingBayE1M1SemanticCatalog.Pickup(state.EntityId); }
            catch (ArgumentOutOfRangeException) { return false; }
            if (state.ItemId != placement.ItemId || state.ProgramId != placement.ProgramId || string.IsNullOrWhiteSpace(state.Cause)) return false;
            if (placement.StartsDormant
                ? state.Lifecycle is not (LoadingBayPickupLifecycle.Dormant or LoadingBayPickupLifecycle.Active or LoadingBayPickupLifecycle.Collected)
                : state.Lifecycle is not (LoadingBayPickupLifecycle.Active or LoadingBayPickupLifecycle.Collected)) return false;
            if (placement.StartsDormant && state.Lifecycle is not LoadingBayPickupLifecycle.Dormant &&
                !LoadingBayE1M1SemanticCatalog.Enemies.Any(enemy => enemy.DropPickupEntityId == placement.EntityId)) return false;
        }
        return true;
    }
    private static bool ValidCooldowns(LoadingBayWeaponCooldownSnapshot[] cooldowns, string[] weapons) =>
        cooldowns.Length <= LoadingBayDefinitions.Weapons.Count
        && cooldowns.All(cooldown => !string.IsNullOrWhiteSpace(cooldown.WeaponId) && weapons.Contains(cooldown.WeaponId, StringComparer.Ordinal))
        && cooldowns.Select(cooldown => cooldown.WeaponId).Distinct(StringComparer.Ordinal).Count() == cooldowns.Length;
    private static bool ValidPlayer(LoadingBayPlayerSnapshot player)
    {
        if (!Finite(player.Position) || !float.IsFinite(player.Look.YawRadians) || !float.IsFinite(player.Look.PitchRadians)) return false;
        if (player.Continuation is null) return true;
        CharacterMotion motion = player.Continuation.Motion;
        return Finite(motion.ControlledVelocity) && Finite(motion.ExternalVelocity) && Enum.IsDefined(motion.Stance)
            && float.IsFinite(motion.JumpBufferRemaining) && float.IsFinite(motion.CoyoteRemaining) && float.IsFinite(motion.LandingLockoutRemaining)
            && Finite(motion.SupportLocalAnchor) && Finite(motion.SupportPreviousTranslation) && Finite(motion.SupportPointVelocity)
            && float.IsFinite(motion.FallOriginY) && float.IsFinite(motion.PeakY)
            && float.IsFinite(motion.SupportPreviousRotation.X) && float.IsFinite(motion.SupportPreviousRotation.Y) && float.IsFinite(motion.SupportPreviousRotation.Z) && float.IsFinite(motion.SupportPreviousRotation.W);
    }
    private static bool Finite(System.Numerics.Vector3 value) => float.IsFinite(value.X) && float.IsFinite(value.Y) && float.IsFinite(value.Z);
    private static bool ValidWeapons(string[] weapons, string? equipped) =>
        ValidDistinct(weapons)
        && weapons.All(LoadingBayDefinitions.Weapons.ContainsKey)
        && weapons.Contains(LoadingBayDefinitions.Fist.Id, StringComparer.Ordinal)
        && (equipped is null || weapons.Contains(equipped, StringComparer.Ordinal));
    private static bool ValidActors(LoadingBayActorSnapshot[] actors)
    {
        if (actors.Length != LoadingBayE1M1SemanticCatalog.Enemies.Length || actors.Select(actor => actor.EntityId).Distinct().Count() != actors.Length) return false;
        foreach (LoadingBayActorSnapshot actor in actors)
        {
            LoadingBayE1M1EnemyDefinition enemy;
            try { enemy = LoadingBayE1M1SemanticCatalog.Enemy(actor.EntityId); }
            catch (InvalidOperationException) { return false; }
            if (actor.Health < 0 || actor.Health > enemy.MaximumHealth || !Enum.IsDefined(actor.Posture)) return false;
            if (actor.Posture == LoadingBayEnemyPosture.Defeated && actor.Health != 0) return false;
            if (actor.Posture != LoadingBayEnemyPosture.Defeated && actor.Health == 0) return false;
        }
        return true;
    }
    private static bool ValidEncounters(LoadingBayEncounterSnapshot[] encounters, LoadingBayActorSnapshot[] actors)
    {
        if (encounters.Length != LoadingBayE1M1SemanticCatalog.Encounters.Length || encounters.Select(encounter => encounter.EntityId).Distinct().Count() != encounters.Length) return false;
        Dictionary<ulong, LoadingBayActorSnapshot> states = actors.ToDictionary(actor => actor.EntityId);
        foreach (LoadingBayEncounterSnapshot encounter in encounters)
        {
            LoadingBayE1M1EncounterDefinition definition;
            try { definition = LoadingBayE1M1SemanticCatalog.Encounters.Single(value => value.EntityId == encounter.EntityId); }
            catch (InvalidOperationException) { return false; }
            if (!encounter.Activated && (encounter.Cleared || definition.Members.Any(member => states[member].Posture != LoadingBayEnemyPosture.Dormant))) return false;
            if (encounter.Cleared != definition.Members.All(member => states[member].Posture == LoadingBayEnemyPosture.Defeated)) return false;
        }
        return true;
    }
    private bool CanApplyPickup(LoadingBayItem item) => item.PickupPolicy switch
    {
        LoadingBayPickupPolicy.Restore(_, var maximum, var consumeAtCap) => _health.Current.Raw < Math.Min(_tuning.MaximumHealth, maximum) || consumeAtCap,
        LoadingBayPickupPolicy.SetMinimum(var minimum, _) => _armor.Current.Raw < minimum,
        LoadingBayPickupPolicy.RestoreArmor(_, var maximum, var consumeAtCap, _) => _armor.Current.Raw < Math.Min(_tuning.MaximumArmor, maximum) || consumeAtCap,
        _ => true,
    };
    private void MaterializeWeapon(LoadingBayWeapon weapon)
    {
        EntityId entity = _entities.Create();
        _inventory.MaterializeUnique(new Mechanics.ItemState(entity, weapon.MechanicsDefinition), _player);
    }
    private void EquipWeapon(string weapon)
    {
        Mechanics.EquipmentService.Equip(_inventory, _player, WeaponEntity(weapon), [LoadingBayDefinitions.WeaponSlot]);
    }
    private string[] OwnedWeaponIds() => _inventory.View(_player).UniqueItems
        .Select(item => WeaponId(item.Definition))
        .OrderBy(id => id, StringComparer.Ordinal)
        .ToArray();
    private string? EquippedWeaponId()
    {
        if (!_inventory.TryGetEquipment(_player, out Mechanics.EquipmentState? equipment)) throw new InvalidOperationException("Player equipment is unavailable.");
        Mechanics.EquipmentAssignment assignment = equipment!.Assignments.SingleOrDefault(value => value.Slot == LoadingBayDefinitions.WeaponSlot.Id);
        return assignment.Slot is null ? null : WeaponIdForEntity(assignment.Item);
    }
    private EntityId WeaponEntity(string weapon) => _inventory.View(_player).UniqueItems
        .Where(item => WeaponId(item.Definition) == weapon)
        .Select(item => item.Entity)
        .Single();
    private string WeaponIdForEntity(EntityId entity)
    {
        if (!_inventory.TryGetItem(entity, out Mechanics.ItemState? item) || item is null) throw new InvalidOperationException("Equipped weapon item is unavailable.");
        return WeaponId(item.Definition.Id);
    }
    private static string WeaponId(Mechanics.ItemDefinitionId definition) => LoadingBayDefinitions.Weapons.Values
        .Single(weapon => weapon.MechanicsDefinition.Id == definition).Id;
    private void RestoreEquippedWeapon(string? desired)
    {
        string? current = EquippedWeaponId();
        if (current == desired) return;
        Mechanics.InventoryWorldCandidate candidate = _inventory.Prepare();
        if (current is null) candidate.Equip(_player, WeaponEntity(desired!), [LoadingBayDefinitions.WeaponSlot]);
        else if (desired is null) candidate.Unequip(_player, WeaponEntity(current));
        else candidate.Swap(_player, WeaponEntity(current), WeaponEntity(desired), [LoadingBayDefinitions.WeaponSlot]);
        candidate.Publish();
    }
    private void RestoreWeapons(string[] desiredWeapons, string? desiredEquipped)
    {
        Dictionary<string, EntityId> current = _inventory.View(_player).UniqueItems
            .ToDictionary(item => WeaponId(item.Definition), item => item.Entity, StringComparer.Ordinal);
        HashSet<string> target = desiredWeapons.ToHashSet(StringComparer.Ordinal);
        List<EntityId> created = [];
        List<EntityId> removed = [];
        try
        {
            Mechanics.InventoryWorldCandidate candidate = _inventory.Prepare();
            string? equipped = EquippedWeaponId();
            if (equipped is not null && !target.Contains(equipped)) candidate.Unequip(_player, current[equipped]);
            foreach ((string weaponId, EntityId entity) in current.Where(pair => !target.Contains(pair.Key)).ToArray())
            {
                candidate.DestroyUnique(entity);
                removed.Add(entity);
                current.Remove(weaponId);
            }
            foreach (string weaponId in target.Where(id => !current.ContainsKey(id)))
            {
                EntityId entity = _entities.Create();
                created.Add(entity);
                candidate.MaterializeUnique(new Mechanics.ItemState(entity, LoadingBayDefinitions.Weapons[weaponId].MechanicsDefinition), _player);
                current.Add(weaponId, entity);
            }
            if (desiredEquipped != equipped)
            {
                if (desiredEquipped is null && equipped is not null && current.ContainsKey(equipped)) candidate.Unequip(_player, current[equipped]);
                else if (desiredEquipped is not null && (equipped is null || !current.ContainsKey(equipped))) candidate.Equip(_player, current[desiredEquipped], [LoadingBayDefinitions.WeaponSlot]);
                else if (desiredEquipped is not null && equipped is not null) candidate.Swap(_player, current[equipped], current[desiredEquipped], [LoadingBayDefinitions.WeaponSlot]);
            }
            candidate.Publish();
            foreach (EntityId entity in removed) if (_entities.IsAlive(entity)) _entities.Destroy(entity);
        }
        catch
        {
            foreach (EntityId entity in created) if (_entities.IsAlive(entity)) _entities.Destroy(entity);
            throw;
        }
    }
    private sealed class EnemyState(int health, LoadingBayEnemyPosture posture, ulong readyAtTick)
    {
        internal int Health { get; set; } = health;
        internal LoadingBayEnemyPosture Posture { get; set; } = posture;
        internal ulong ReadyAtTick { get; set; } = readyAtTick;
        internal bool Visible { get; set; }
    }
    private sealed class PersistenceOnlyContext(IPersistenceService persistence) : IEngineContext
    {
        public IDiagnosticsService Diagnostics => throw new NotSupportedException();
        public ILookService Look => throw new NotSupportedException();
        public IDynamicsService Dynamics => throw new NotSupportedException();
        public IMotionService Motion => throw new NotSupportedException();
        public IKinematicService Kinematic => throw new NotSupportedException();
        public ISpatialService Spatial => throw new NotSupportedException();
        public IPerceptionService Perception => throw new NotSupportedException();
        public IWorldOriginService WorldOrigin => throw new NotSupportedException();
        public IVoxelService Voxel => throw new NotSupportedException();
        public IVoxelContentService VoxelContent => throw new NotSupportedException();
        public IContentService Content => throw new NotSupportedException();
        public IAuthoredContentService AuthoredContent => throw new NotSupportedException();
        public IAppearanceService Appearance => throw new NotSupportedException();
        public IPresentationService Presentation => throw new NotSupportedException();
        public IAnimationService Animation => throw new NotSupportedException();
        public IAudioService Audio => throw new NotSupportedException();
        public ICameraViewService CameraView => throw new NotSupportedException();
        public IRandomService Random => throw new NotSupportedException();
        public IVoxelScenePresentationService VoxelScenePresentation => throw new NotSupportedException();
        public IPersistenceService Persistence { get; } = persistence;
        public IContentStoreService ContentStore => throw new NotSupportedException();
        public IUiService Ui => throw new NotSupportedException();
    }
    private void ThrowIfDisposed() { if (_disposed) throw new ObjectDisposedException(nameof(LoadingBaySession)); }
}
