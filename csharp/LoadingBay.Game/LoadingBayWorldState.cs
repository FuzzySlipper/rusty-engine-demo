using System.Numerics;

namespace LoadingBay.Game;

/// <summary>Typed, saveable E1M1 progression meaning. Engine owns the trigger/query and motion realizations.</summary>
internal enum LoadingBayDoorState { Closed, Opening, Open, Closing }
internal enum LoadingBayFloorState { Armed, Lowering, Lowered }
internal enum LoadingBayLiftState { Raised, Lowering, Waiting, Raising }
internal sealed record LoadingBayDoorSnapshot(ulong EntityId, LoadingBayDoorState State, ulong DueStep);
internal sealed record LoadingBayFloorSnapshot(ulong EntityId, LoadingBayFloorState State, ulong DueStep);
internal sealed record LoadingBayLiftSnapshot(ulong EntityId, LoadingBayLiftState State, ulong DueStep);
internal sealed record LoadingBayBarrelSnapshot(ulong EntityId, int Health, bool Exploded);
internal sealed record LoadingBayHazardSnapshot(ulong EntityId, ulong ReadyAtStep);
internal sealed record LoadingBayWorldSnapshot(LoadingBayDoorSnapshot[] Doors, LoadingBayFloorSnapshot[] Floors, LoadingBayLiftSnapshot[] Lifts, LoadingBayBarrelSnapshot[] Barrels, LoadingBayHazardSnapshot[] Hazards);
internal sealed record WorldInteractionFact(string Family, ulong EntityId, string State, ulong Tick, ulong DueStep) : LoadingBayFact;
internal sealed record BarrelExplosionFact(ulong BarrelEntityId, int Damage, double Radius, ulong Tick, bool Chained) : LoadingBayFact;

/// <summary>Product policy only: state transitions, due steps, and canonical E1M1 values remain explicit and inspectable.</summary>
internal sealed class LoadingBayWorldState
{
    private readonly Dictionary<ulong, LoadingBayDoorSnapshot> _doors = [];
    private readonly Dictionary<ulong, LoadingBayFloorSnapshot> _floors = [];
    private readonly Dictionary<ulong, LoadingBayLiftSnapshot> _lifts = [];
    private readonly Dictionary<ulong, LoadingBayBarrelSnapshot> _barrels = [];
    private readonly Dictionary<ulong, ulong> _hazardReadyAt = [];

    internal LoadingBayWorldState()
    {
        foreach (LoadingBayE1M1DoorDefinition door in LoadingBayE1M1SemanticCatalog.Doors) _doors.Add(door.EntityId, new(door.EntityId, LoadingBayDoorState.Closed, 0));
        foreach (LoadingBayE1M1FloorDefinition floor in LoadingBayE1M1SemanticCatalog.Floors) _floors.Add(floor.EntityId, new(floor.EntityId, LoadingBayFloorState.Armed, 0));
        foreach (LoadingBayE1M1LiftDefinition lift in LoadingBayE1M1SemanticCatalog.Lifts) _lifts.Add(lift.EntityId, new(lift.EntityId, LoadingBayLiftState.Raised, 0));
        foreach (LoadingBayE1M1BarrelDefinition barrel in LoadingBayE1M1SemanticCatalog.Barrels) _barrels.Add(barrel.EntityId, new(barrel.EntityId, barrel.MaximumHealth, false));
        foreach (LoadingBayE1M1HazardDefinition hazard in LoadingBayE1M1SemanticCatalog.Hazards) _hazardReadyAt.Add(hazard.EntityId, 0);
    }

    internal LoadingBayWorldSnapshot Capture() => new(
        _doors.Values.OrderBy(value => value.EntityId).ToArray(), _floors.Values.OrderBy(value => value.EntityId).ToArray(),
        _lifts.Values.OrderBy(value => value.EntityId).ToArray(), _barrels.Values.OrderBy(value => value.EntityId).ToArray(),
        _hazardReadyAt.OrderBy(value => value.Key).Select(value => new LoadingBayHazardSnapshot(value.Key, value.Value)).ToArray());

    internal bool TryRestore(LoadingBayWorldSnapshot snapshot)
    {
        if (!Same(snapshot.Doors, _doors.Keys, value => value.EntityId) || !Same(snapshot.Floors, _floors.Keys, value => value.EntityId) || !Same(snapshot.Lifts, _lifts.Keys, value => value.EntityId) || !Same(snapshot.Barrels, _barrels.Keys, value => value.EntityId) || snapshot.Hazards.Length != _hazardReadyAt.Count || snapshot.Hazards.Select(value => value.EntityId).Distinct().Count() != snapshot.Hazards.Length || snapshot.Hazards.Any(value => !_hazardReadyAt.ContainsKey(value.EntityId))) return false;
        if (snapshot.Doors.Any(value => !Enum.IsDefined(value.State)) || snapshot.Floors.Any(value => !Enum.IsDefined(value.State)) || snapshot.Lifts.Any(value => !Enum.IsDefined(value.State))) return false;
        if (snapshot.Doors.Any(value => value.State == LoadingBayDoorState.Closed ? value.DueStep != 0 : value.DueStep == 0)
            || snapshot.Floors.Any(value => value.State is LoadingBayFloorState.Armed or LoadingBayFloorState.Lowered ? value.DueStep != 0 : value.DueStep == 0)
            || snapshot.Lifts.Any(value => value.State == LoadingBayLiftState.Raised ? value.DueStep != 0 : value.DueStep == 0)) return false;
        if (snapshot.Barrels.Any(value => value.Health < 0 || value.Health > LoadingBayE1M1SemanticCatalog.Barrels.Single(definition => definition.EntityId == value.EntityId).MaximumHealth || (value.Exploded && value.Health != 0))) return false;
        Replace(_doors, snapshot.Doors, value => value.EntityId); Replace(_floors, snapshot.Floors, value => value.EntityId); Replace(_lifts, snapshot.Lifts, value => value.EntityId); Replace(_barrels, snapshot.Barrels, value => value.EntityId);
        foreach (LoadingBayHazardSnapshot cooldown in snapshot.Hazards) _hazardReadyAt[cooldown.EntityId] = cooldown.ReadyAtStep;
        return true;
    }

    internal bool HazardReady(ulong hazardEntityId, ulong tick) => _hazardReadyAt.TryGetValue(hazardEntityId, out ulong due) && tick >= due;
    internal LoadingBayE1M1HazardDefinition ApplyHazard(ulong hazardEntityId, ulong tick, Action<int, string> damage, Action<LoadingBayFact> record)
    {
        LoadingBayE1M1HazardDefinition hazard = LoadingBayE1M1SemanticCatalog.Hazards.Single(value => value.EntityId == hazardEntityId);
        if (!HazardReady(hazardEntityId, tick)) throw new InvalidOperationException("Hazard cooldown is not ready.");
        ulong due = checked(tick + (ulong)hazard.CooldownTicks);
        _hazardReadyAt[hazardEntityId] = due;
        damage(hazard.Damage, $"hazard.{hazard.Label}");
        record(new WorldInteractionFact("hazard", hazard.EntityId, "cooldown", tick, due));
        return hazard;
    }

    internal LoadingBayDoorSnapshot ActivateDoor(ulong entityId, ulong tick, Action<LoadingBayFact> record)
    {
        LoadingBayE1M1DoorDefinition definition = LoadingBayE1M1SemanticCatalog.Doors.Single(value => value.EntityId == entityId);
        LoadingBayDoorSnapshot state = _doors[entityId];
        if (state.State is LoadingBayDoorState.Opening or LoadingBayDoorState.Open) return state;
        state = new(entityId, LoadingBayDoorState.Opening, checked(tick + (ulong)definition.MotionDurationTicks)); _doors[entityId] = state;
        record(new WorldInteractionFact("door", entityId, "opening", tick, state.DueStep)); return state;
    }

    internal LoadingBayFloorSnapshot ActivateFloor(ulong entityId, ulong tick, Action<LoadingBayFact> record)
    {
        LoadingBayE1M1FloorDefinition definition = LoadingBayE1M1SemanticCatalog.Floors.Single(value => value.EntityId == entityId);
        LoadingBayFloorSnapshot state = _floors[entityId];
        if (state.State != LoadingBayFloorState.Armed) return state;
        state = new(entityId, LoadingBayFloorState.Lowering, checked(tick + (ulong)definition.MotionDurationTicks)); _floors[entityId] = state;
        record(new WorldInteractionFact("floor", entityId, "lowering", tick, state.DueStep)); return state;
    }

    internal LoadingBayLiftSnapshot ActivateLift(ulong entityId, ulong tick, Action<LoadingBayFact> record)
    {
        LoadingBayE1M1LiftDefinition definition = LoadingBayE1M1SemanticCatalog.Lifts.Single(value => value.EntityId == entityId);
        LoadingBayLiftSnapshot state = _lifts[entityId];
        if (state.State is LoadingBayLiftState.Lowering or LoadingBayLiftState.Waiting) return state;
        state = new(entityId, LoadingBayLiftState.Lowering, checked(tick + (ulong)definition.MotionDurationTicks)); _lifts[entityId] = state;
        record(new WorldInteractionFact("lift", entityId, "lowering", tick, state.DueStep)); return state;
    }

    internal void Advance(ulong tick, Action<LoadingBayFact> record)
    {
        foreach (LoadingBayDoorSnapshot state in _doors.Values.Where(value => value.DueStep != 0 && value.DueStep <= tick).ToArray())
        {
            LoadingBayE1M1DoorDefinition definition = LoadingBayE1M1SemanticCatalog.Doors.Single(value => value.EntityId == state.EntityId);
            LoadingBayDoorSnapshot next = state.State switch
            {
                LoadingBayDoorState.Opening => new(state.EntityId, LoadingBayDoorState.Open, checked(tick + (ulong)definition.AutoCloseAfterTicks)),
                LoadingBayDoorState.Open => new(state.EntityId, LoadingBayDoorState.Closing, checked(tick + (ulong)definition.MotionDurationTicks)),
                LoadingBayDoorState.Closing => new(state.EntityId, LoadingBayDoorState.Closed, 0),
                _ => state,
            };
            _doors[state.EntityId] = next; record(new WorldInteractionFact("door", state.EntityId, next.State.ToString().ToLowerInvariant(), tick, next.DueStep));
        }
        foreach (LoadingBayFloorSnapshot state in _floors.Values.Where(value => value.DueStep != 0 && value.DueStep <= tick).ToArray())
        {
            LoadingBayFloorSnapshot next = new(state.EntityId, LoadingBayFloorState.Lowered, 0); _floors[state.EntityId] = next;
            record(new WorldInteractionFact("floor", state.EntityId, "lowered", tick, 0));
        }
        foreach (LoadingBayLiftSnapshot state in _lifts.Values.Where(value => value.DueStep != 0 && value.DueStep <= tick).ToArray())
        {
            LoadingBayE1M1LiftDefinition definition = LoadingBayE1M1SemanticCatalog.Lifts.Single(value => value.EntityId == state.EntityId);
            LoadingBayLiftSnapshot next = state.State switch
            {
                LoadingBayLiftState.Lowering => new(state.EntityId, LoadingBayLiftState.Waiting, checked(tick + (ulong)definition.LoweredWaitTicks)),
                LoadingBayLiftState.Waiting => new(state.EntityId, LoadingBayLiftState.Raising, checked(tick + (ulong)definition.MotionDurationTicks)),
                LoadingBayLiftState.Raising => new(state.EntityId, LoadingBayLiftState.Raised, 0),
                _ => state,
            };
            _lifts[state.EntityId] = next; record(new WorldInteractionFact("lift", state.EntityId, next.State.ToString().ToLowerInvariant(), tick, next.DueStep));
        }
    }

    internal IEnumerable<LoadingBayE1M1BarrelDefinition> DamageBarrel(ulong entityId, int damage, ulong tick, Func<LoadingBayE1M1BarrelDefinition, LoadingBayE1M1BarrelDefinition, bool> occluded, Action<LoadingBayFact> record)
    {
        if (damage <= 0) return [];
        ArgumentNullException.ThrowIfNull(occluded);
        // Build all damage waves and all Engine line-of-effect queries against a detached candidate.
        // A failed query therefore cannot leave one barrel committed while later members of its chain are unresolved.
        Dictionary<ulong, LoadingBayBarrelSnapshot> staged = _barrels.ToDictionary(value => value.Key, value => value.Value);
        Queue<(ulong Id, int Damage, bool Chained)> pending = new(); pending.Enqueue((entityId, damage, false)); List<(LoadingBayE1M1BarrelDefinition Barrel, bool Chained)> exploded = [];
        while (pending.TryDequeue(out (ulong Id, int Damage, bool Chained) item))
        {
            if (!staged.TryGetValue(item.Id, out LoadingBayBarrelSnapshot? state) || state.Exploded) continue;
            int health = Math.Max(0, state.Health - item.Damage); staged[item.Id] = state with { Health = health };
            if (health != 0) continue;
            LoadingBayE1M1BarrelDefinition barrel = LoadingBayE1M1SemanticCatalog.Barrels.Single(value => value.EntityId == item.Id);
            staged[item.Id] = new(item.Id, 0, true); exploded.Add((barrel, item.Chained));
            foreach (LoadingBayE1M1BarrelDefinition candidate in LoadingBayE1M1SemanticCatalog.Barrels.Where(value => value.EntityId != barrel.EntityId))
            {
                double distance = Vector3.Distance(barrel.Translation, candidate.Translation);
                if (distance > barrel.Radius || occluded(barrel, candidate)) continue;
                int scaled = (int)Math.Ceiling(barrel.Damage * (1d - distance / barrel.Radius));
                if (scaled > 0) pending.Enqueue((candidate.EntityId, scaled, true));
            }
        }
        _barrels.Clear(); foreach ((ulong id, LoadingBayBarrelSnapshot state) in staged) _barrels.Add(id, state);
        foreach ((LoadingBayE1M1BarrelDefinition barrel, bool chained) in exploded) record(new BarrelExplosionFact(barrel.EntityId, barrel.Damage, barrel.Radius, tick, chained));
        return exploded.Select(value => value.Barrel).ToArray();
    }

    /// <summary>Semantic continuations only; Engine scheduler handles are deliberately not part of product persistence.</summary>
    internal IEnumerable<ulong> DueSteps() => _doors.Values.Concat<object>(_floors.Values).Concat(_lifts.Values)
        .Select(value => value switch { LoadingBayDoorSnapshot door => door.DueStep, LoadingBayFloorSnapshot floor => floor.DueStep, LoadingBayLiftSnapshot lift => lift.DueStep, _ => 0UL })
        .Where(value => value != 0).Distinct();

    private static bool Same<T>(IReadOnlyCollection<T> values, ICollection<ulong> keys, Func<T, ulong> id) => values.Count == keys.Count && values.Select(id).Distinct().Count() == values.Count && values.All(value => keys.Contains(id(value)));
    private static void Replace<T>(Dictionary<ulong, T> destination, IEnumerable<T> values, Func<T, ulong> id) { foreach (T value in values) destination[id(value)] = value; }
}
