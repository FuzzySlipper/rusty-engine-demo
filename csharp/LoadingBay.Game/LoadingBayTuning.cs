using System.Numerics;
using Rusty.Engine;

namespace LoadingBay.Game;

/// <summary>Immutable E1M1 policy retained beside the product domains that consume it.</summary>
internal sealed record LoadingBayTuning(
    int StartingHealth,
    int MaximumHealth,
    int StartingArmor,
    int MaximumArmor,
    ulong InventorySlots,
    int FactJournalCapacity,
    uint MaximumPickupBindings,
    uint MaximumPickupFactReadback,
    uint MaximumSpatialEntityBindings,
    Vector3 PlayerPickupHalfExtents,
    string ContentIdentity,
    float MovementSpeed,
    float LookDegreesPerUnit,
    Vector3 AuthoredPlayerPosition,
    float AuthoredPlayerKinematicHalfHeight,
    float StandingCharacterHeight,
    float CrouchedCharacterHeight,
    float CharacterRadius,
    float AuthoredBaseEyeHeight,
    float InitialYawDegrees,
    float InitialPitchDegrees,
    float Gravity,
    float JumpSpeed,
    float MaximumStepHeight,
    double CameraFieldOfViewDegrees,
    double CameraNearPlane,
    double CameraFarPlane,
    double SpatialVoxelSize,
    uint SpatialChunkSize,
    LoadingBayE1M1Landmark ExitLandmark,
    LoadingBayE1M1AnimationCue ExitButtonCue,
    double PerceptionMaximumDistance,
    double PerceptionMinimumFacingCosine,
    float ProjectileLinearDamping,
    float ProjectileAngularDamping,
    float ProjectileFriction,
    float EffectsVolume,
    bool EffectsMuted)
{
    /// <summary>Engine character center lift required to preserve the authored E1M1 base-position convention.</summary>
    internal float EngineCenterLift => (StandingCharacterHeight * .5f) - AuthoredPlayerKinematicHalfHeight;

    /// <summary>Initial character center consumed by Engine Spatial; retained for existing pose call sites.</summary>
    internal Vector3 InitialEngineCenter => AuthoredPlayerPosition + (Vector3.UnitY * EngineCenterLift);

    internal Vector3 InitialPosition => InitialEngineCenter;

    /// <summary>Camera/perception elevation relative to the Engine character center.</summary>
    internal float EyeOffsetFromCenter => AuthoredBaseEyeHeight - EngineCenterLift;

    internal static LoadingBayTuning E1M1 { get; } = new(
        100, 200, 0, 200, 10, 32, MaximumPickupBindings: 78, MaximumPickupFactReadback: 156, MaximumSpatialEntityBindings: 94, PlayerPickupHalfExtents: new Vector3(.25f, .5f, .25f), ContentIdentity: "doom-e1m1",
        MovementSpeed: 6f,
        LookDegreesPerUnit: 12f,
        // Canonical player node from the committed E1M1 project. Spatial receives a
        // character center, so retain the authored base and conversion inputs separately.
        AuthoredPlayerPosition: new Vector3(114f, 9.5f, 78f),
        AuthoredPlayerKinematicHalfHeight: .25f,
        StandingCharacterHeight: 1.8f,
        CrouchedCharacterHeight: 1.1f,
        CharacterRadius: .25f,
        AuthoredBaseEyeHeight: 2.0625f,
        InitialYawDegrees: 180f,
        InitialPitchDegrees: -6f,
        Gravity: 24f,
        JumpSpeed: 8f,
        MaximumStepHeight: 1.5f,
        CameraFieldOfViewDegrees: 90d,
        CameraNearPlane: .1d,
        CameraFarPlane: 1_024d,
        SpatialVoxelSize: 1d,
        SpatialChunkSize: 16,
        // Canonical scene node 149, label `doom-exit`, from the committed E1M1 project.
        ExitLandmark: new LoadingBayE1M1Landmark("doom-exit", 149, new Vector3(230f, 7.5f, 6f)),
        ExitButtonCue: new LoadingBayE1M1AnimationCue(
            "doom-exit",
            149,
            "doom-e1m1/props/sources/kenney-factory-kit/button-floor-square.glb",
            "toggle-off",
            "toggle-on",
            new Transform(new Vector3(230f, 7.5f, 6f), Quaternion.Identity, Vector3.One),
            1f,
            1f,
            1f),
        PerceptionMaximumDistance: 512d,
        PerceptionMinimumFacingCosine: 0.35d,
        ProjectileLinearDamping: 0f,
        ProjectileAngularDamping: 0f,
        ProjectileFriction: 0f,
        EffectsVolume: 0.8f,
        EffectsMuted: false);
}

/// <summary>Named E1M1 authored cue used by product policy while Engine performs the spatial work.</summary>
internal readonly record struct LoadingBayE1M1Landmark(string Id, ulong EntityId, Vector3 Position);

/// <summary>Named authored placement and direct embedded-clip policy for the E1M1 exit button.</summary>
internal readonly record struct LoadingBayE1M1AnimationCue(
    string Id,
    ulong ObjectId,
    string SourcePath,
    string OffClip,
    string OnClip,
    Transform Transform,
    float PlaybackSpeed,
    float PlaybackWeight,
    float OffNormalizedTime);
