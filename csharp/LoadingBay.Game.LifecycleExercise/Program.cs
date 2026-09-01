using System.Numerics;
using LoadingBay.Game;
using Rusty.Engine;
using Rusty.Engine.Entities;
using Rusty.Engine.Persistence;

var created = new List<RecordingSession>(); int attempts = 0;
Require(new[]
    {
        LoadingBayAdmittedStepTicks.At(Update(step: 12, admittedSteps: 3).Facts, 0),
        LoadingBayAdmittedStepTicks.At(Update(step: 12, admittedSteps: 3).Facts, 1),
        LoadingBayAdmittedStepTicks.At(Update(step: 12, admittedSteps: 3).Facts, 2),
    }.SequenceEqual([12UL, 13UL, 14UL]), "batched admitted movement did not reconcile every host-admitted simulation tick");
Require(new[]
    {
        LoadingBayAdmittedStepTicks.At(Update(step: 0, admittedSteps: 4).Facts, 0),
        LoadingBayAdmittedStepTicks.At(Update(step: 0, admittedSteps: 4).Facts, 1),
        LoadingBayAdmittedStepTicks.At(Update(step: 0, admittedSteps: 4).Facts, 2),
        LoadingBayAdmittedStepTicks.At(Update(step: 0, admittedSteps: 4).Facts, 3),
    }.SequenceEqual([0UL, 1UL, 2UL, 3UL]), "first catch-up batch did not preserve Engine-admitted tick identities");
bool admittedTickOverflowRejected = false;
try { _ = LoadingBayAdmittedStepTicks.At(Update(step: ulong.MaxValue, admittedSteps: 2).Facts, 1); }
catch (OverflowException) { admittedTickOverflowRejected = true; }
Require(admittedTickOverflowRejected, "admitted movement tick overflow was not rejected");
using (var supportWorld = new EntityWorld([EngineComponentTypes.Transform, EngineComponentTypes.Kinematic, EngineComponentTypes.SpatialCollider]))
{
    EntityId movingLift = supportWorld.Create();
    Transform advancedLiftTransform = new(new Vector3(112f, 6f, 76f), Quaternion.Identity, Vector3.One);
    supportWorld.Set(movingLift, EngineComponentTypes.Transform, advancedLiftTransform);
    supportWorld.Set(movingLift, EngineComponentTypes.Kinematic, new Kinematic(new Vector3(10f, 2f, 6f), new Vector3(0f, -1f, 0f)));
    supportWorld.Set(movingLift, EngineComponentTypes.SpatialCollider, new SpatialCollider(new Vector3(-10f, -2f, -6f), new Vector3(10f, 2f, 6f), 2, uint.MaxValue, true, false, false));
    CharacterSupport continuation = LoadingBayWorldInteractionCoordinator.ResolvePlatformSupport(
        true, movingLift.Value, new HashSet<ulong> { movingLift.Value }, supportWorld);
    CharacterObstacle obstacle = LoadingBayWorldInteractionCoordinator.ProjectPlatformObstacles(
        new HashSet<ulong> { movingLift.Value }, supportWorld).Single();
    Require(continuation.Present && continuation.Lifecycle == CharacterSupportLifecycle.Active
        && continuation.Entity == movingLift.Value && continuation.Transform == advancedLiftTransform
        && obstacle.Entity == movingLift.Value && obstacle.Transform == advancedLiftTransform
        && obstacle.LinearVelocity == new Vector3(0f, -1f, 0f),
        "E1M1 platform carry did not supply the post-motion Engine support continuation");
}
var product = new LoadingBayProduct(() =>
{
    attempts++; if (attempts == 3) throw new InvalidOperationException("expected replacement failure");
    var session = new RecordingSession(); created.Add(session); return session;
});
Require(product.Update(Update()) == ProductUpdateResult.None && created[0].UpdateCount == 0, "updates before Start must be ignored");
product.Start(); product.Update(Update()); Require(created[0].UpdateCount == 1, "running update was not delegated");
product.Restart(); Require(created.Count == 2 && created[0].DisposeCount == 1 && created[1].PublishCount == 1, "restart did not replace after fresh publication");
bool replacementFailed = false; try { product.Restart(); } catch (InvalidOperationException) { replacementFailed = true; }
Require(replacementFailed && created[1].DisposeCount == 0, "failed replacement did not retain the current session");
product.Update(Update()); Require(created[1].UpdateCount == 1, "current session was lost after failed replacement"); product.Shutdown(); Require(created[1].DisposeCount == 1, "shutdown did not dispose active replacement");

var disposalSessions = new List<RecordingSession>(); int disposalAttempts = 0;
var disposalProduct = new LoadingBayProduct(() =>
{
    RecordingSession session = disposalAttempts++ == 0 ? new ThrowingDisposeSession() : new RecordingSession();
    disposalSessions.Add(session);
    return session;
});
disposalProduct.Start();
disposalProduct.Pause();
disposalProduct.Restart();
Require(disposalSessions[0].DisposeCount == 1, "restart did not attempt old-session disposal");
disposalProduct.Update(Update());
Require(disposalSessions[1].UpdateCount == 1, "replacement was not authoritative after old-session disposal failed");
bool reportedRetirementFailure = false;
try { disposalProduct.Shutdown(); }
catch (AggregateException exception) { reportedRetirementFailure = exception.InnerExceptions.Any(candidate => candidate.Message == "expected old-session disposal failure"); }
Require(reportedRetirementFailure && disposalSessions[1].DisposeCount == 1, "shutdown did not report old retirement failure after disposing the active replacement");

var realSessions = new List<LoadingBaySession>();
using (var realProduct = new LoadingBayProduct(() => { var session = new LoadingBaySession(); realSessions.Add(session); return session; }))
{
    realProduct.Start(); realProduct.Update(Update(step: 9)); realSessions[0].ApplyDamage("player", 15, "restart-proof");
    realProduct.Restart();
    LoadingBayReadout fresh = realSessions[1].Readout();
    Require(realSessions[0].Readout().Health < fresh.Health && fresh.Generation == 0 && fresh.Facts.Length == 1, "restart retained stale gameplay state, facts, or generation");
}

LoadingBaySession state = new();
LoadingBayTuning spawnTuning = LoadingBayTuning.E1M1;
Require(spawnTuning.AuthoredPlayerPosition == new Vector3(114f, 9.5f, 78f)
    && MathF.Abs(spawnTuning.EngineCenterLift - .65f) < .0001f
    && spawnTuning.InitialEngineCenter == new Vector3(114f, 10.15f, 78f)
    && MathF.Abs(spawnTuning.EyeOffsetFromCenter - 1.4125f) < .0001f
    && MathF.Abs(spawnTuning.InitialEngineCenter.Y + spawnTuning.EyeOffsetFromCenter - (spawnTuning.AuthoredPlayerPosition.Y + spawnTuning.AuthoredBaseEyeHeight)) < .0001f,
    "E1M1 authored-base to Engine-center spawn conversion drifted");
LoadingBaySnapshot preFirstStep = state.Capture("doom-e1m1");
Require(preFirstStep.Player.Continuation is null && state.Restore(preFirstStep, "doom-e1m1").Accepted
    && state.Capture("doom-e1m1").Player.Continuation is null,
    "pre-first-step snapshot did not retain its explicit no-continuation state");
state.Update(Update(step: 1));
Require(state.Readout().Player.Value == 1, "E1M1 did not retain canonical player identity 1 after entity bootstrap");
Require(LoadingBayTuning.E1M1.MaximumSpatialEntityBindings == 94 && LoadingBayE1M1SemanticCatalog.Floors.Single().PlatformBoundsMin != LoadingBayE1M1SemanticCatalog.Floors.Single().BoundsMin
    && LoadingBayE1M1SemanticCatalog.Lifts.Single().PlatformBoundsMax != LoadingBayE1M1SemanticCatalog.Lifts.Single().BoundsMax,
    "generated world target platform bounds or shared spatial bound drifted");
Require(state.ApplyCanonicalHazard(137, 1).Accepted && !state.ApplyCanonicalHazard(137, 2).Accepted && state.ApplyCanonicalHazard(137, 56).Accepted, "canonical nukage did not retain its inclusive cooldown boundary");
Require(state.ActivateCanonicalDoor(141, 1).Accepted, "canonical door did not enter opening state");
Require(state.ActivateCanonicalFloor(146, 1).Accepted && state.ActivateCanonicalLift(148, 1).Accepted, "canonical floor or lift did not begin lowering");
LoadingBaySnapshot worldStart = state.Capture("doom-e1m1");
Require(worldStart.World.Doors.Single(door => door.EntityId == 141).State == LoadingBayDoorState.Opening
    && worldStart.World.Floors.Single(floor => floor.EntityId == 146).State == LoadingBayFloorState.Lowering
    && worldStart.World.Lifts.Single(lift => lift.EntityId == 148).State == LoadingBayLiftState.Lowering,
    "world snapshot did not retain typed in-flight transitions");
state.Update(Update(step: 59));
state.Update(Update(step: 66));
LoadingBaySnapshot lowered = state.Capture("doom-e1m1");
Require(lowered.World.Doors.Single(door => door.EntityId == 141).State == LoadingBayDoorState.Open
    && lowered.World.Floors.Single(floor => floor.EntityId == 146).State == LoadingBayFloorState.Lowered
    && lowered.World.Lifts.Single(lift => lift.EntityId == 148).State == LoadingBayLiftState.Waiting,
    "canonical due steps did not settle the door/floor/lift boundaries");
Require(state.Restore(worldStart, "doom-e1m1").Accepted
    && state.Capture("doom-e1m1").World.Doors.Single(door => door.EntityId == 141).State == LoadingBayDoorState.Opening,
    "world snapshot restore did not rebuild semantic in-flight state");
Require(!state.Restore(worldStart with { World = worldStart.World with { Floors = [worldStart.World.Floors[0] with { State = LoadingBayFloorState.Lowered, DueStep = 1 }] } }, "doom-e1m1").Accepted,
    "world snapshot accepted an impossible settled floor due step");
Require(state.DamageCanonicalBarrel(60, 20, 60).Accepted && state.Capture("doom-e1m1").World.Barrels.Single(barrel => barrel.EntityId == 60).Exploded,
    "canonical barrel did not settle through the typed explosive state");
Require(state.DiscoverCanonicalSecret(150).Accepted && !state.DiscoverCanonicalSecret(150).Accepted, "canonical secret was not one-shot");
Require(state.CompleteCanonicalExit(149).Accepted, "canonical exit was incorrectly encounter-gated");
Require(state.Readout().OwnedWeapons.SequenceEqual([LoadingBayDefinitions.Fist.Id, LoadingBayDefinitions.Pistol.Id], StringComparer.Ordinal), "E1M1 did not own fist and pistol");
Require(state.Readout().EquippedWeapon == LoadingBayDefinitions.Pistol.Id && state.Readout().Bullets == 50, "E1M1 equipment or bullets drifted");
Require(state.DeveloperSetTrack(1, "health", 99, "canonical-pickup").Accepted, "exercise could not establish a health-bonus delta");
Require(state.CollectCanonicalPickup(78).Accepted && state.Readout().Health == 100, "canonical health pickup did not use generated E1M1 semantics");
Require(!state.CollectCanonicalPickup(78).Accepted, "canonical health pickup collected more than once");
LoadingBaySnapshot collectedSnapshot = state.Capture("doom-e1m1");
Require(state.ApplyDamage("player", 20, "post-pickup").Accepted && state.Restore(collectedSnapshot, "doom-e1m1").Accepted && state.Readout().Health == 100, "canonical pickup snapshot did not restore its health result");
Require(!state.CollectPickup("medikit-full", LoadingBayDefinitions.Medikit, 1).Accepted, "full-health medikit was consumed");
Require(state.ApplyDamage("player", 30, "exercise").Accepted, "damage was rejected");
Require(state.CollectPickup("medikit-full", LoadingBayDefinitions.Medikit, 1).Accepted, "rejected medikit was incorrectly retired");
Require(state.DeveloperSetTrack(1, "armor", 90, "exercise").Accepted, "exercise could not establish partial green armor");
Require(state.CollectPickup("armor-1", LoadingBayDefinitions.GreenArmor, 1).Accepted && state.Readout().Armor == 100, "green armor did not set 90 armor to its minimum");
Require(!state.CollectPickup("armor-full", LoadingBayDefinitions.GreenArmor, 1).Accepted, "unneeded green armor was consumed");
LoadingBaySnapshot greenArmorSnapshot = state.Capture("doom-e1m1");
long greenHealth = state.Readout().Health;
Require(state.ApplyDamage("player", 9, "green-armor").Accepted && state.Readout().Health == greenHealth - 6 && state.Readout().ArmorProtection.Mode == LoadingBayArmorProtectionMode.Green && state.Readout().ArmorProtection.AbsorptionDivisor == 3, "green armor did not retain its typed divisor");
Require(state.DeveloperSetTrack(1, "armor", 0, "bonus").Accepted && state.CollectPickup("armor-bonus", LoadingBayDefinitions.ArmorBonus, 1).Accepted && state.Readout().ArmorProtection.Mode == LoadingBayArmorProtectionMode.Bonus, "armor bonus did not select its typed protection mode");
Require(state.DeveloperSetTrack(1, "armor", 9, "bonus-divisor").Accepted, "exercise could not establish bonus armor protection");
long bonusHealth = state.Readout().Health;
Require(state.ApplyDamage("player", 9, "bonus-armor").Accepted && state.Readout().Health == bonusHealth - 6, "armor bonus did not use divisor three");
Require(state.CollectPickup("armor-blue", LoadingBayDefinitions.BlueArmor, 1).Accepted && state.Readout().ArmorProtection.Mode == LoadingBayArmorProtectionMode.Blue, "blue armor did not select its typed protection mode");
Require(state.CollectPickup("armor-bonus-after-blue", LoadingBayDefinitions.ArmorBonus, 1).Accepted && state.Readout().ArmorProtection.Mode == LoadingBayArmorProtectionMode.Blue && state.Readout().ArmorProtection.AbsorptionDivisor == 2, "armor bonus replaced blue armor protection instead of preserving it");
long blueHealth = state.Readout().Health;
Require(state.ApplyDamage("player", 10, "blue-armor").Accepted && state.Readout().Health == blueHealth - 5, "blue armor did not use divisor two");
Require(state.Restore(greenArmorSnapshot, "doom-e1m1").Accepted && state.Readout().ArmorProtection.Mode == LoadingBayArmorProtectionMode.Green && state.Readout().ArmorProtection.AbsorptionDivisor == 3, "snapshot restore did not preserve typed armor protection");
Require(state.CollectCanonicalPickup(108).Accepted && state.Readout().OwnedWeapons.Contains(LoadingBayDefinitions.Shotgun.Id, StringComparer.Ordinal) && state.Readout().Shells == 8, "authored shotgun pickup did not atomically grant equipment and starter shells");
Require(!state.CollectCanonicalPickup(25).Accepted, "dormant enemy shotgun drop was collectable before its owner materialized it");
Require(state.Restore(collectedSnapshot, "doom-e1m1").Accepted && !state.Readout().OwnedWeapons.Contains(LoadingBayDefinitions.Shotgun.Id, StringComparer.Ordinal) && state.Readout().Shells == 0, "snapshot restore did not remove later-acquired Engine equipment and shells");
Require(state.CollectPickup("shell-cap", LoadingBayDefinitions.Shells, LoadingBayDefinitions.Shells.MechanicsDefinition.MaximumQuantity).Accepted && !state.CanCollectCanonicalPickup(108) && !state.CollectCanonicalPickup(108).Accepted && !state.Readout().OwnedWeapons.Contains(LoadingBayDefinitions.Shotgun.Id, StringComparer.Ordinal), "starter-ammo overflow staged a weapon or retired its pickup");
LoadingBaySnapshot snapshot = state.Capture("doom-e1m1"); state.ApplyDamage("player", 20, "exercise");
Require(snapshot.Player.Position == LoadingBayTuning.E1M1.InitialPosition
    && snapshot.Player.Look.YawRadians == -(LoadingBayTuning.E1M1.InitialYawDegrees * (MathF.PI / 180f))
    && snapshot.Pickups.Length == LoadingBayE1M1SemanticCatalog.Pickups.Length,
    "semantic snapshot omitted canonical player pose/look or typed pickup state");
Require(!state.Restore(snapshot with { WeaponCooldowns = [new LoadingBayWeaponCooldownSnapshot("weapon/not-owned", 1)] }, "doom-e1m1").Accepted,
    "snapshot accepted a cooldown for an unowned weapon");
Require(!state.Restore(snapshot with { EquippedWeapon = "weapon/not-owned" }, "doom-e1m1").Accepted, "invalid equipped weapon was accepted");
long beforeImpossibleEncounter = state.Readout().Health;
Require(!state.Restore(snapshot with { Encounters = [snapshot.Encounters[0] with { Activated = false, Cleared = true }, .. snapshot.Encounters[1..]] }, "doom-e1m1").Accepted && state.Readout().Health == beforeImpossibleEncounter, "impossible encounter activation state was accepted or mutated gameplay");
Require(state.Restore(snapshot with { EquippedWeapon = LoadingBayDefinitions.Fist.Id }, "doom-e1m1").Accepted && state.Readout().EquippedWeapon == LoadingBayDefinitions.Fist.Id, "restore did not swap the actual Engine equipment slot");
Require(state.Restore(snapshot, "doom-e1m1").Accepted, "valid snapshot did not restore atomically");
Require(!state.Restore(snapshot with { Pickups = [.. snapshot.Pickups[..^1], snapshot.Pickups[^1] with { ItemId = "pickup/00078" }] }, "doom-e1m1").Accepted, "snapshot accepted a noncanonical pickup alias");
LoadingBayE1M1EnemyDefinition hitscanEnemy = LoadingBayE1M1SemanticCatalog.Enemies.First(enemy => enemy.AttackKind == LoadingBayE1M1EnemyAttackKind.Hitscan);
LoadingBayE1M1EncounterDefinition hitscanEncounter = LoadingBayE1M1SemanticCatalog.Encounters.First(encounter => encounter.Members.Contains(hitscanEnemy.EntityId));
Require(state.ActivateEncounter(hitscanEncounter.EntityId, 70).Accepted, "canonical hitscan encounter did not activate");
Require(!state.PrepareEnemyAttacks(70, new HashSet<ulong>(), 0, 0).Any(plan => plan.EnemyEntityId == hitscanEnemy.EntityId), "distance-rejected enemy without a Perception pair was treated as visible");
LoadingBayEnemyAttackPlan hitscanPlan = state.PrepareEnemyAttacks(70, new HashSet<ulong> { hitscanEnemy.EntityId }, 1, 0).Single(plan => plan.EnemyEntityId == hitscanEnemy.EntityId);
long beforeEnemyAttack = state.Readout().Health;
Require(state.SettleEnemyAttack(hitscanPlan, true, "exercise.hitscan").Accepted && state.Readout().Health == beforeEnemyAttack - hitscanEnemy.AttackDamage, "Engine-backed enemy attack settlement did not apply typed canonical damage");
Require(!state.PrepareEnemyAttacks(71, new HashSet<ulong> { hitscanEnemy.EntityId }, 1, 0).Any(plan => plan.EnemyEntityId == hitscanEnemy.EntityId), "enemy cooldown admitted a repeated attack early");
LoadingBayE1M1EncounterDefinition combatEncounter = LoadingBayE1M1SemanticCatalog.Encounters.First(encounter => encounter.EntityId != hitscanEncounter.EntityId);
Require(state.ActivateEncounter(combatEncounter.EntityId, 80).Accepted, "canonical encounter did not activate its members");
foreach (ulong enemyId in combatEncounter.Members)
{
    LoadingBayE1M1EnemyDefinition enemy = LoadingBayE1M1SemanticCatalog.Enemy(enemyId);
    Require(state.ApplyWeaponDamage(enemyId, LoadingBayDefinitions.Fist.Id, enemy.MaximumHealth, 81).Accepted, "canonical enemy did not accept an admitted lethal hit");
}
Require(state.Capture("doom-e1m1").Pickups.Where(pickup => pickup.Lifecycle == LoadingBayPickupLifecycle.Active).Any(pickup => pickup.Cause == "enemy.drop-materialized") && state.Readout().Facts.OfType<EnemyDefeatedFact>().Any() && state.Readout().Facts.OfType<EncounterChangedFact>().Any(fact => fact.Encounter == combatEncounter.Label && fact.Cleared), "same-tick enemy defeat did not materialize its dormant drop state and clear the encounter");
LoadingBaySnapshot canonicalCombatSnapshot = state.Capture("doom-e1m1");
Require(canonicalCombatSnapshot.Actors.Length == LoadingBayE1M1SemanticCatalog.Enemies.Length
    && canonicalCombatSnapshot.Encounters.Length == LoadingBayE1M1SemanticCatalog.Encounters.Length
    && canonicalCombatSnapshot.Encounters.Single(encounter => encounter.EntityId == combatEncounter.EntityId) is { Activated: true, Cleared: true }
    && canonicalCombatSnapshot.Actors.Where(actor => combatEncounter.Members.Contains(actor.EntityId)).All(actor => actor.Posture == LoadingBayEnemyPosture.Defeated && actor.Health == 0), "canonical actor/encounter snapshot did not capture activation and defeat state");
Require(state.Restore(canonicalCombatSnapshot, "doom-e1m1").Accepted && state.Readout().Enemies.Where(enemy => combatEncounter.Members.Contains(enemy.EntityId)).All(enemy => enemy.Posture == LoadingBayEnemyPosture.Defeated), "canonical actor snapshot did not restore defeated actor state");
ulong bulletDrop = combatEncounter.Members.Select(LoadingBayE1M1SemanticCatalog.Enemy)
    .First(enemy => enemy.DropPickupEntityId != 0 && LoadingBayE1M1SemanticCatalog.Pickup(enemy.DropPickupEntityId).ItemId == LoadingBayDefinitions.Bullets.Id).DropPickupEntityId;
LoadingBaySnapshot activeDropSnapshot = state.Capture("doom-e1m1");
Require(state.CanCollectCanonicalPickup(bulletDrop) && state.CollectCanonicalPickup(bulletDrop).Accepted && state.Readout().Bullets == 55, "defeated enemy's activated dormant drop was not collectible through the canonical pickup policy");
Require(state.Restore(activeDropSnapshot, "doom-e1m1").Accepted && state.CanCollectCanonicalPickup(bulletDrop), "snapshot restore did not retain an active canonical enemy drop lifecycle");
for (int index = 0; index < 40; index++) _ = state.CollectCanonicalPickup(78);
Require(state.Readout().DroppedFacts > 0 && state.Readout().Facts.OfType<RejectedFact>().Any(fact => fact.Code == "pickup.already-collected"), "bounded pickup fact journal lost its repeated-collection observability");
state.Dispose();

var persistence = new InMemoryPersistenceService(); using var persisted = new LoadingBaySession(persistence); persisted.Update(Update(step: 7)); persisted.ApplyDamage("player", 12, "persistence"); long savedHealth = persisted.Readout().Health;
Require(persisted.Save("e1m1").Accepted, "Engine ProductStateStore did not save"); persisted.ApplyDamage("player", 20, "mutation"); Require(persisted.Load("e1m1").Accepted && persisted.Readout().Health == savedHealth, "Engine ProductStateStore did not restore");
persistence.Seed("loading-bay", "corrupt", LoadingBaySnapshotCodec.CurrentSchema, [0xff]); long beforeCorrupt = persisted.Readout().Health; bool corruptRejected = false; try { persisted.Load("corrupt"); } catch (Exception) { corruptRejected = true; }
Require(corruptRejected && persisted.Readout().Health == beforeCorrupt, "corrupt persistence mutated state"); persistence.Seed("loading-bay", "old", 1, []); bool oldRejected = false; try { persisted.Load("old"); } catch (InvalidOperationException) { oldRejected = true; }
Require(oldRejected && persisted.Readout().Health == beforeCorrupt, "old schema was accepted or mutated state"); Console.WriteLine("Loading Bay lifecycle exercise passed.");

static ProductUpdate Update(ulong step = 1, uint admittedSteps = 1) => new(new ProductUpdateFacts(ProductUpdateMode.Demand, ProductLifecycleState.Running, 1, 1, 0, step, 0, admittedSteps, 0, 0), ReadOnlySpan<ProductInputEvent>.Empty);
static void Require(bool condition, string message) { if (!condition) throw new InvalidOperationException(message); }
file class RecordingSession : ILoadingBaySession
{
    public int UpdateCount { get; private set; }
    public int PublishCount { get; private set; }
    public int DisposeCount { get; private set; }

    public ProductUpdateResult Update(ProductUpdate update)
    {
        UpdateCount++;
        return ProductUpdateResult.None;
    }

    public void Publish() => PublishCount++;
    public void ActivateSharedRealizations() { }
    public void DeactivateSharedRealizations() { }
    public virtual void Dispose() => DisposeCount++;

    public LoadingBayReadout Readout() =>
        throw new InvalidOperationException("The lifecycle fake has no gameplay readout.");

    public LoadingBayReceipt DeveloperSetTrack(ulong generation, string track, int value, string correlation) =>
        throw new InvalidOperationException("The lifecycle fake has no developer mutation surface.");
}
file sealed class ThrowingDisposeSession : RecordingSession { public override void Dispose() { base.Dispose(); throw new InvalidOperationException("expected old-session disposal failure"); } }
file sealed class InMemoryPersistenceService : IPersistenceService
{
    private sealed record Saved(uint SchemaVersion, ulong Revision, byte[] Payload); private readonly Dictionary<ulong, string> _scopes = []; private readonly Dictionary<ulong, Saved> _blobs = []; private readonly Dictionary<(string Scope, string Key), Saved> _saved = []; private ulong _next = 1;
    public PersistenceStore OpenStore(PersistenceOpenRequest request) { ulong handle = _next++; _scopes.Add(handle, request.Scope); return new(new PersistenceStoreHandle(handle), () => _scopes.Remove(handle)); }
    public PersistenceSaveReceipt Save(PersistenceSaveRequest request) { var key = (_scopes[request.Store.Handle.Value], request.Key); _saved.TryGetValue(key, out Saved? old); ulong revision = (old?.Revision ?? 0) + 1; _saved[key] = new(request.SchemaVersion, revision, request.Payload.ToArray()); return new(revision, request.SchemaVersion); }
    public PersistenceBlob Load(PersistenceLoadRequest request) { _saved.TryGetValue((_scopes[request.Store.Handle.Value], request.Key), out Saved? saved); ulong handle = _next++; _blobs.Add(handle, saved ?? new(0, 0, [])); return new(new PersistenceBlobHandle(handle), () => _blobs.Remove(handle)); }
    public PersistenceBlobInfo DescribeBlob(PersistenceBlob blob) { Saved saved = _blobs[blob.Handle.Value]; return new(saved.Revision != 0, saved.SchemaVersion, saved.Revision, (nuint)saved.Payload.Length); }
    public void CopyBlob(PersistenceCopyBlobRequest request) => _blobs[request.Blob.Handle.Value].Payload.CopyTo(request.Destination.Span);
    public ReadOnlyMemory<byte> ReadBlobBytes(PersistenceBlob blob) => _blobs[blob.Handle.Value].Payload;
    public void Seed(string scope, string key, uint schema, byte[] payload) => _saved[(scope, key)] = new(schema, 1, payload);
}
