using System.Numerics;
using Rusty.Engine;
using Rusty.Engine.Entities;
using Mechanics = Rusty.Engine.Mechanics;

namespace LoadingBay.Game;

internal enum LoadingBayItemKind { Ammunition, Health, Armor }
internal enum LoadingBayArmorProtectionMode { None, Blue, Green, Bonus }
internal enum LoadingBayPickupLifecycle { Dormant, Active, Collected }
internal enum LoadingBayEnemyPosture { Dormant, Active, Pained, Defeated }
internal readonly record struct LoadingBayArmorProtection(LoadingBayArmorProtectionMode Mode, int AbsorptionDivisor)
{
    internal static readonly LoadingBayArmorProtection None = new(LoadingBayArmorProtectionMode.None, 0);
}
internal abstract record LoadingBayWorldAction
{
    internal sealed record EncounterActivated(string Encounter, string[] Members) : LoadingBayWorldAction;
    internal sealed record EnemyDefeated(string Enemy) : LoadingBayWorldAction;
    internal sealed record HazardApplied(int Damage, string Cause) : LoadingBayWorldAction;
    internal sealed record BarrelExploded(int Damage) : LoadingBayWorldAction;
    internal sealed record FloorActivated(string Floor) : LoadingBayWorldAction;
    internal sealed record LiftActivated(string Lift) : LoadingBayWorldAction;
    internal sealed record SwitchActivated(string Door) : LoadingBayWorldAction;
}
internal abstract record LoadingBayPickupPolicy
{
    internal sealed record Restore(int Amount, int Maximum, bool ConsumeAtCap) : LoadingBayPickupPolicy;
    internal sealed record SetMinimum(int Value, LoadingBayArmorProtection Protection) : LoadingBayPickupPolicy;
    internal sealed record RestoreArmor(int Amount, int Maximum, bool ConsumeAtCap, LoadingBayArmorProtection Protection) : LoadingBayPickupPolicy;
}
internal sealed record LoadingBayItem(string Id, LoadingBayItemKind Kind, Mechanics.ItemDefinition MechanicsDefinition, LoadingBayPickupPolicy? PickupPolicy = null);
internal sealed record LoadingBayWeapon(string Id, Mechanics.ItemDefinition MechanicsDefinition);
internal readonly record struct LoadingBayReceipt(bool Accepted, string Code, string? Correlation);
internal sealed record LoadingBayPickupSnapshot(ulong EntityId, string ItemId, string ProgramId, LoadingBayPickupLifecycle Lifecycle, string Cause, ulong Tick, ulong TriggerRevision);
internal sealed record LoadingBayEnemyReadout(ulong EntityId, string Label, int Health, LoadingBayEnemyPosture Posture, bool Visible, ulong ReadyAtTick, ulong DropPickupEntityId);
/// <summary>Copied identity and authored surface values for one Engine-projected E1M1 material.</summary>
internal sealed record LoadingBayAuthoredMaterialReadout(
    string MaterialId,
    uint MaterialVersion,
    string MaterialHash,
    uint MaterialSlot,
    ulong MaterialHandle,
    string TextureId,
    uint TextureVersion,
    string TextureHash,
    string TextureSourcePath,
    AuthoredUvStrategy UvStrategy,
    AuthoredVoxelSurfaceMappingKind MappingKind,
    float TileScaleX,
    float TileScaleY,
    float TileOriginX,
    float TileOriginY,
    string AtlasId,
    string Region,
    AuthoredTextureFilter Filter,
    AuthoredTextureWrap Wrap);
/// <summary>Typed catalog identity plus Engine voxel-scene realization diagnostics.</summary>
internal sealed record LoadingBayVoxelSceneReadout(
    string CatalogPath,
    string CatalogHash,
    uint CatalogEntryCount,
    uint MaterialCount,
    uint BoundMaterialCount,
    uint MappingCount,
    ulong MappingSourceRevision,
    ulong MappingMeshRevision,
    bool Realized,
    ulong SceneSourceRevision,
    ulong SceneMeshRevision,
    ulong SceneChunkCount,
    LoadingBayAuthoredMaterialReadout[] Materials)
{
    internal static LoadingBayVoxelSceneReadout Empty => new(
        string.Empty, string.Empty, 0, 0, 0, 0, 0, 0, false, 0, 0, 0, []);
}
/// <summary>Retained authored SKY1 identity and camera-view realization state.</summary>
internal readonly record struct LoadingBaySkyReadout(
    string SourcePath,
    ContentSha256 SourceHash,
    ulong SourceByteLength,
    ulong ResourceHandle,
    bool ResourceRealized,
    bool BackgroundSelected)
{
    internal static LoadingBaySkyReadout Empty => new(string.Empty, default, 0, 0, false, false);
}
internal sealed record LoadingBayWeaponFirePlan(string WeaponId, string AmmunitionId, int AmmunitionCost, int DamageRolls, int Damage, int PelletCount, double SpreadDegrees, double MaximumDistance, ulong Tick);
internal sealed record LoadingBayWeaponImpact(ulong EnemyEntityId, int Damage, int PelletIndex, bool WorldOccluded);
/// <summary>One product-authorized enemy action; Engine performs the visibility, casts, and body simulation.</summary>
internal sealed record LoadingBayEnemyAttackPlan(ulong EnemyEntityId, LoadingBayE1M1EnemyAttackKind Kind, Vector3 Origin, int Damage, double Range, int CooldownTicks, float ProjectileMass, float ProjectileRadius, float ProjectileImpulse, float ProjectileGravityScale, int ProjectileLifetimeTicks, float ProjectileRestitution, ulong Tick);
/// <summary>Product-owned copy of the public Engine continuation checkpoint; it contains values, never leases or handles.</summary>
internal sealed record LoadingBayCharacterContinuationSnapshot(ulong SourceSessionIdentity, ulong SourceGeneration, ulong SpatialSessionFingerprint, ulong ContentAuthorityHash, ulong ConfigFingerprint, CharacterControllerConfig Config, CharacterMotion Motion);
/// <summary>Canonical pose and look plus an optional post-step continuation. Null means the snapshot predates the first admitted character step.</summary>
internal sealed record LoadingBayPlayerSnapshot(Vector3 Position, LookState Look, LoadingBayCharacterContinuationSnapshot? Continuation);
internal sealed record LoadingBayWeaponCooldownSnapshot(string WeaponId, ulong ReadyAtTick);
internal sealed record LoadingBaySnapshot(string ContentIdentity, long Health, long Armor, LoadingBayArmorProtection ArmorProtection, ulong Bullets, ulong Shells, string[] OwnedWeapons, string? EquippedWeapon, LoadingBayWeaponCooldownSnapshot[] WeaponCooldowns, LoadingBayPlayerSnapshot Player, LoadingBayPickupSnapshot[] Pickups, string[] Secrets, bool Complete, LoadingBayNamedState[] Doors, LoadingBayActorSnapshot[] Actors, LoadingBayEncounterSnapshot[] Encounters, LoadingBayWorldSnapshot World);
internal readonly record struct LoadingBayNamedState(string Id, bool Value);
internal sealed record LoadingBayActorSnapshot(ulong EntityId, int Health, LoadingBayEnemyPosture Posture, bool Visible, ulong ReadyAtTick);
internal sealed record LoadingBayEncounterSnapshot(ulong EntityId, bool Activated, bool Cleared);
internal sealed record LoadingBayReadout(EntityId Player, ProductUpdateFacts UpdateFacts, long Health, long Armor, LoadingBayArmorProtection ArmorProtection, ulong Bullets, ulong Shells, string[] OwnedWeapons, string? EquippedWeapon, LoadingBayWeaponCooldownSnapshot[] WeaponCooldowns, LoadingBayPlayerSnapshot PlayerState, LoadingBayPickupSnapshot[] Pickups, LoadingBayEnemyReadout[] Enemies, bool Complete, uint PendingSchedules, LoadingBayTuning Tuning, LoadingBayFact[] Facts, ulong DroppedFacts)
{
    internal ulong Generation => UpdateFacts.Generation;
    internal ulong Step => UpdateFacts.SimulationStep;
}
internal abstract record LoadingBayFact;
internal sealed record SessionStartedFact(EntityId Player) : LoadingBayFact;
internal sealed record PickupCollectedFact(string Pickup, string Item, ulong Quantity) : LoadingBayFact;
internal sealed record DamageAppliedFact(string Target, int Requested, long ArmorAbsorbed, long HealthApplied, string Cause, bool Defeated) : LoadingBayFact;
internal sealed record DoorChangedFact(string Door, bool Open) : LoadingBayFact;
internal sealed record EncounterChangedFact(string Encounter, bool Cleared) : LoadingBayFact;
internal sealed record WorldActionFact(string Code, string Subject) : LoadingBayFact;
internal sealed record SecretDiscoveredFact(string Secret) : LoadingBayFact;
internal sealed record ExitCompletedFact(string Exit) : LoadingBayFact;
internal sealed record DeveloperTrackChangedFact(string Track, int Value, string Correlation) : LoadingBayFact;
internal sealed record SnapshotRestoredFact(string Identity) : LoadingBayFact;
internal sealed record SemanticInputFact(string Intent) : LoadingBayFact;
internal sealed record CanonicalPickupOverlapFact(ulong PickupEntityId, ulong SubjectEntityId, ulong Tick, bool Accepted, string Code) : LoadingBayFact;
internal sealed record CanonicalPickupTriggerStateFact(ulong PickupEntityId, bool Active, ulong RevisionBefore, ulong RevisionAfter, uint OverlapCount, string Cause) : LoadingBayFact;
internal sealed record PickupLoadoutChangedFact(ulong PickupEntityId, string ItemId, string ProgramId, bool Active, string Code) : LoadingBayFact;
internal sealed record PickupLifecycleFact(ulong PickupEntityId, string ItemId, string ProgramId, LoadingBayPickupLifecycle Lifecycle, string Cause, ulong Tick, ulong TriggerRevision) : LoadingBayFact;
internal sealed record EnemyPostureChangedFact(ulong EnemyEntityId, LoadingBayEnemyPosture Posture, int Health, ulong Tick, string Cause) : LoadingBayFact;
internal sealed record EnemyHitFact(ulong EnemyEntityId, string WeaponId, int Damage, int RemainingHealth, ulong Tick) : LoadingBayFact;
internal sealed record EnemyDefeatedFact(ulong EnemyEntityId, ulong DropPickupEntityId, ulong Tick) : LoadingBayFact;
internal sealed record EnemyPerceptionFact(ulong EnemyEntityId, bool Visible, ulong Tick, uint VisibilityCasts, uint OcclusionRejects) : LoadingBayFact;
internal sealed record EnemyAttackFact(ulong EnemyEntityId, LoadingBayE1M1EnemyAttackKind Kind, bool HitPlayer, ulong Tick, string Cause) : LoadingBayFact;
internal sealed record EnemyProjectileFact(ulong EnemyEntityId, ulong Tick, string Cause) : LoadingBayFact;
internal sealed record EncounterActivatedFact(ulong EncounterEntityId, string Encounter, double Radius, ulong Tick) : LoadingBayFact;
internal sealed record WeaponFiredFact(string WeaponId, ulong Tick, int PelletCount, int ImpactCount) : LoadingBayFact;
internal sealed record WeaponMissedFact(string WeaponId, ulong Tick, int PelletIndex, string Cause) : LoadingBayFact;
internal sealed record RejectedFact(string Code, string? Correlation) : LoadingBayFact;
