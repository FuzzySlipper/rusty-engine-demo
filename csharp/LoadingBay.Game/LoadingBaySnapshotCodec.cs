using System.Buffers;
using System.Numerics;
using System.Text;
using Rusty.Engine;
using Rusty.Engine.Persistence;

namespace LoadingBay.Game;

/// <summary>Versioned binary persistence meaning for the state owned by this slice.</summary>
internal sealed class LoadingBaySnapshotCodec : IProductStateCodec<LoadingBaySnapshot>
{
    internal const uint CurrentSchema = 8;
    public uint SchemaVersion => CurrentSchema;

    public void Encode(in LoadingBaySnapshot state, IBufferWriter<byte> destination)
    {
        using var stream = new MemoryStream();
        using (var writer = new BinaryWriter(stream, Encoding.UTF8, leaveOpen: true))
        {
            writer.Write(state.ContentIdentity); writer.Write(state.Health); writer.Write(state.Armor); writer.Write((int)state.ArmorProtection.Mode); writer.Write(state.ArmorProtection.AbsorptionDivisor); writer.Write(state.Bullets); writer.Write(state.Shells); writer.Write(state.Complete);
            Write(writer, state.OwnedWeapons); writer.Write(state.EquippedWeapon is not null); if (state.EquippedWeapon is not null) writer.Write(state.EquippedWeapon);
            Write(writer, state.WeaponCooldowns); Write(writer, state.Player);
            Write(writer, state.Pickups); Write(writer, state.Secrets); Write(writer, state.Doors); Write(writer, state.Actors); Write(writer, state.Encounters); Write(writer, state.World);
        }
        destination.Write(stream.GetBuffer().AsSpan(0, checked((int)stream.Length)));
    }

    public LoadingBaySnapshot Decode(ReadOnlySpan<byte> payload)
    {
        using var reader = new BinaryReader(new MemoryStream(payload.ToArray()), Encoding.UTF8, leaveOpen: false);
        string identity = reader.ReadString(); long health = reader.ReadInt64(); long armor = reader.ReadInt64(); LoadingBayArmorProtection protection = new((LoadingBayArmorProtectionMode)reader.ReadInt32(), reader.ReadInt32()); ulong bullets = reader.ReadUInt64(); ulong shells = reader.ReadUInt64(); bool complete = reader.ReadBoolean();
        string[] weapons = Read(reader); string? equipped = reader.ReadBoolean() ? reader.ReadString() : null;
        LoadingBayWeaponCooldownSnapshot[] cooldowns = ReadCooldowns(reader); LoadingBayPlayerSnapshot player = ReadPlayer(reader);
        LoadingBayPickupSnapshot[] pickups = ReadPickups(reader); string[] secrets = Read(reader); LoadingBayNamedState[] doors = ReadState(reader); LoadingBayActorSnapshot[] actors = ReadActors(reader); LoadingBayEncounterSnapshot[] encounters = ReadEncounters(reader); LoadingBayWorldSnapshot world = ReadWorld(reader);
        if (reader.BaseStream.Position != reader.BaseStream.Length) throw new InvalidOperationException("Loading Bay snapshot has trailing bytes.");
        return new LoadingBaySnapshot(identity, health, armor, protection, bullets, shells, weapons, equipped, cooldowns, player, pickups, secrets, complete, doors, actors, encounters, world);
    }
    private static void Write(BinaryWriter writer, string[] values) { writer.Write(values.Length); foreach (string value in values) writer.Write(value); }
    private static void Write(BinaryWriter writer, LoadingBayWeaponCooldownSnapshot[] values) { writer.Write(values.Length); foreach (LoadingBayWeaponCooldownSnapshot value in values) { writer.Write(value.WeaponId); writer.Write(value.ReadyAtTick); } }
    private static void Write(BinaryWriter writer, LoadingBayPlayerSnapshot value)
    {
        Write(writer, value.Position); writer.Write(value.Look.YawRadians); writer.Write(value.Look.PitchRadians);
        writer.Write(value.Continuation is not null);
        if (value.Continuation is not null) Write(writer, value.Continuation);
    }
    private static void Write(BinaryWriter writer, LoadingBayCharacterContinuationSnapshot value)
    {
        writer.Write(value.SourceSessionIdentity); writer.Write(value.SourceGeneration); writer.Write(value.SpatialSessionFingerprint); writer.Write(value.ContentAuthorityHash); writer.Write(value.ConfigFingerprint); Write(writer, value.Config);
        CharacterMotion motion = value.Motion;
        Write(writer, motion.ControlledVelocity); Write(writer, motion.ExternalVelocity); writer.Write(motion.Grounded); writer.Write((int)motion.Stance);
        writer.Write(motion.JumpBufferRemaining); writer.Write(motion.CoyoteRemaining); writer.Write(motion.LandingLockoutRemaining);
        writer.Write(motion.SupportEntityPresent); writer.Write(motion.SupportEntity); Write(writer, motion.SupportLocalAnchor); Write(writer, motion.SupportPreviousTranslation); Write(writer, motion.SupportPreviousRotation); Write(writer, motion.SupportPointVelocity);
        writer.Write(motion.FallOriginY); writer.Write(motion.PeakY); writer.Write(motion.LastCommandSequence); writer.Write(motion.CollisionWorldHash);
    }
    private static void Write(BinaryWriter writer, LoadingBayPickupSnapshot[] values) { writer.Write(values.Length); foreach (LoadingBayPickupSnapshot value in values) { writer.Write(value.EntityId); writer.Write(value.ItemId); writer.Write(value.ProgramId); writer.Write((int)value.Lifecycle); writer.Write(value.Cause); writer.Write(value.Tick); writer.Write(value.TriggerRevision); } }
    private static void Write(BinaryWriter writer, LoadingBayNamedState[] values) { writer.Write(values.Length); foreach (LoadingBayNamedState value in values) { writer.Write(value.Id); writer.Write(value.Value); } }
    private static void Write(BinaryWriter writer, LoadingBayActorSnapshot[] values) { writer.Write(values.Length); foreach (LoadingBayActorSnapshot value in values) { writer.Write(value.EntityId); writer.Write(value.Health); writer.Write((int)value.Posture); writer.Write(value.Visible); writer.Write(value.ReadyAtTick); } }
    private static void Write(BinaryWriter writer, LoadingBayEncounterSnapshot[] values) { writer.Write(values.Length); foreach (LoadingBayEncounterSnapshot value in values) { writer.Write(value.EntityId); writer.Write(value.Activated); writer.Write(value.Cleared); } }
    private static void Write(BinaryWriter writer, LoadingBayWorldSnapshot world) { Write(writer, world.Doors); Write(writer, world.Floors); Write(writer, world.Lifts); Write(writer, world.Barrels); writer.Write(world.Hazards.Length); foreach (LoadingBayHazardSnapshot value in world.Hazards) { writer.Write(value.EntityId); writer.Write(value.ReadyAtStep); } }
    private static void Write(BinaryWriter writer, LoadingBayDoorSnapshot[] values) { writer.Write(values.Length); foreach (LoadingBayDoorSnapshot value in values) { writer.Write(value.EntityId); writer.Write((int)value.State); writer.Write(value.DueStep); } }
    private static void Write(BinaryWriter writer, LoadingBayFloorSnapshot[] values) { writer.Write(values.Length); foreach (LoadingBayFloorSnapshot value in values) { writer.Write(value.EntityId); writer.Write((int)value.State); writer.Write(value.DueStep); } }
    private static void Write(BinaryWriter writer, LoadingBayLiftSnapshot[] values) { writer.Write(values.Length); foreach (LoadingBayLiftSnapshot value in values) { writer.Write(value.EntityId); writer.Write((int)value.State); writer.Write(value.DueStep); } }
    private static void Write(BinaryWriter writer, LoadingBayBarrelSnapshot[] values) { writer.Write(values.Length); foreach (LoadingBayBarrelSnapshot value in values) { writer.Write(value.EntityId); writer.Write(value.Health); writer.Write(value.Exploded); } }
    private static string[] Read(BinaryReader reader) { int count = reader.ReadInt32(); if (count is < 0 or > 256) throw new InvalidOperationException("Invalid snapshot collection count."); return Enumerable.Range(0, count).Select(_ => reader.ReadString()).ToArray(); }
    private static LoadingBayWeaponCooldownSnapshot[] ReadCooldowns(BinaryReader reader) { int count = reader.ReadInt32(); if (count is < 0 or > 16) throw new InvalidOperationException("Invalid snapshot cooldown count."); return Enumerable.Range(0, count).Select(_ => new LoadingBayWeaponCooldownSnapshot(reader.ReadString(), reader.ReadUInt64())).ToArray(); }
    private static LoadingBayPlayerSnapshot ReadPlayer(BinaryReader reader)
    {
        Vector3 position = ReadVector3(reader); LookState look = new(reader.ReadSingle(), reader.ReadSingle());
        return new LoadingBayPlayerSnapshot(position, look, reader.ReadBoolean() ? ReadContinuation(reader) : null);
    }
    private static LoadingBayCharacterContinuationSnapshot ReadContinuation(BinaryReader reader)
    {
        ulong session = reader.ReadUInt64(); ulong generation = reader.ReadUInt64(); ulong spatial = reader.ReadUInt64(); ulong content = reader.ReadUInt64(); ulong fingerprint = reader.ReadUInt64(); CharacterControllerConfig config = ReadConfig(reader);
        CharacterMotion motion = new(ReadVector3(reader), ReadVector3(reader), reader.ReadBoolean(), (CharacterStance)reader.ReadInt32(), reader.ReadSingle(), reader.ReadSingle(), reader.ReadSingle(), reader.ReadBoolean(), reader.ReadUInt64(), ReadVector3(reader), ReadVector3(reader), ReadQuaternion(reader), ReadVector3(reader), reader.ReadSingle(), reader.ReadSingle(), reader.ReadUInt64(), reader.ReadUInt64());
        return new LoadingBayCharacterContinuationSnapshot(session, generation, spatial, content, fingerprint, config, motion);
    }
    private static LoadingBayPickupSnapshot[] ReadPickups(BinaryReader reader) { int count = reader.ReadInt32(); if (count is < 0 or > 256) throw new InvalidOperationException("Invalid snapshot collection count."); return Enumerable.Range(0, count).Select(_ => new LoadingBayPickupSnapshot(reader.ReadUInt64(), reader.ReadString(), reader.ReadString(), (LoadingBayPickupLifecycle)reader.ReadInt32(), reader.ReadString(), reader.ReadUInt64(), reader.ReadUInt64())).ToArray(); }
    private static LoadingBayNamedState[] ReadState(BinaryReader reader) { int count = reader.ReadInt32(); if (count is < 0 or > 256) throw new InvalidOperationException("Invalid snapshot collection count."); return Enumerable.Range(0, count).Select(_ => new LoadingBayNamedState(reader.ReadString(), reader.ReadBoolean())).ToArray(); }
    private static LoadingBayActorSnapshot[] ReadActors(BinaryReader reader) { int count = reader.ReadInt32(); if (count is < 0 or > 256) throw new InvalidOperationException("Invalid snapshot collection count."); return Enumerable.Range(0, count).Select(_ => new LoadingBayActorSnapshot(reader.ReadUInt64(), reader.ReadInt32(), (LoadingBayEnemyPosture)reader.ReadInt32(), reader.ReadBoolean(), reader.ReadUInt64())).ToArray(); }
    private static LoadingBayEncounterSnapshot[] ReadEncounters(BinaryReader reader) { int count = reader.ReadInt32(); if (count is < 0 or > 256) throw new InvalidOperationException("Invalid snapshot collection count."); return Enumerable.Range(0, count).Select(_ => new LoadingBayEncounterSnapshot(reader.ReadUInt64(), reader.ReadBoolean(), reader.ReadBoolean())).ToArray(); }
    private static LoadingBayWorldSnapshot ReadWorld(BinaryReader reader) => new(ReadDoors(reader), ReadFloors(reader), ReadLifts(reader), ReadBarrels(reader), ReadHazards(reader));
    private static int Count(BinaryReader reader) { int count = reader.ReadInt32(); if (count is < 0 or > 256) throw new InvalidOperationException("Invalid snapshot collection count."); return count; }
    private static LoadingBayDoorSnapshot[] ReadDoors(BinaryReader reader) => Enumerable.Range(0, Count(reader)).Select(_ => new LoadingBayDoorSnapshot(reader.ReadUInt64(), (LoadingBayDoorState)reader.ReadInt32(), reader.ReadUInt64())).ToArray();
    private static LoadingBayFloorSnapshot[] ReadFloors(BinaryReader reader) => Enumerable.Range(0, Count(reader)).Select(_ => new LoadingBayFloorSnapshot(reader.ReadUInt64(), (LoadingBayFloorState)reader.ReadInt32(), reader.ReadUInt64())).ToArray();
    private static LoadingBayLiftSnapshot[] ReadLifts(BinaryReader reader) => Enumerable.Range(0, Count(reader)).Select(_ => new LoadingBayLiftSnapshot(reader.ReadUInt64(), (LoadingBayLiftState)reader.ReadInt32(), reader.ReadUInt64())).ToArray();
    private static LoadingBayBarrelSnapshot[] ReadBarrels(BinaryReader reader) => Enumerable.Range(0, Count(reader)).Select(_ => new LoadingBayBarrelSnapshot(reader.ReadUInt64(), reader.ReadInt32(), reader.ReadBoolean())).ToArray();
    private static LoadingBayHazardSnapshot[] ReadHazards(BinaryReader reader) => Enumerable.Range(0, Count(reader)).Select(_ => new LoadingBayHazardSnapshot(reader.ReadUInt64(), reader.ReadUInt64())).ToArray();
    private static void Write(BinaryWriter writer, Vector3 value) { writer.Write(value.X); writer.Write(value.Y); writer.Write(value.Z); }
    private static Vector3 ReadVector3(BinaryReader reader) => new(reader.ReadSingle(), reader.ReadSingle(), reader.ReadSingle());
    private static void Write(BinaryWriter writer, Quaternion value) { writer.Write(value.X); writer.Write(value.Y); writer.Write(value.Z); writer.Write(value.W); }
    private static Quaternion ReadQuaternion(BinaryReader reader) => new(reader.ReadSingle(), reader.ReadSingle(), reader.ReadSingle(), reader.ReadSingle());

    private static void Write(BinaryWriter w, CharacterControllerConfig c) { Write(w, c.Shape); Write(w, c.Ground); Write(w, c.Air); Write(w, c.Vertical); Write(w, c.Jump); Write(w, c.Surface); Write(w, c.Recovery); Write(w, c.Platform); Write(w, c.ExternalMotion); Write(w, c.Solver); }
    private static CharacterControllerConfig ReadConfig(BinaryReader r) => new(ReadShape(r), ReadGround(r), ReadAir(r), ReadVertical(r), ReadJump(r), ReadSurface(r), ReadRecovery(r), ReadPlatform(r), ReadExternal(r), ReadSolver(r));
    private static void Write(BinaryWriter w, CharacterShapeConfig c) { w.Write(c.StandingHeight); w.Write(c.CrouchedHeight); w.Write(c.Radius); w.Write(c.ContactSkin); w.Write(c.ClearancePadding); }
    private static CharacterShapeConfig ReadShape(BinaryReader r) => new(r.ReadSingle(), r.ReadSingle(), r.ReadSingle(), r.ReadSingle(), r.ReadSingle());
    private static void Write(BinaryWriter w, CharacterGroundConfig c) { w.Write(c.ForwardSpeed); w.Write(c.BackwardSpeed); w.Write(c.StrafeSpeed); w.Write(c.Acceleration); w.Write(c.Braking); w.Write(c.Friction); w.Write(c.StopSpeed); w.Write(c.DirectionChangeMultiplier); }
    private static CharacterGroundConfig ReadGround(BinaryReader r) => new(r.ReadSingle(), r.ReadSingle(), r.ReadSingle(), r.ReadSingle(), r.ReadSingle(), r.ReadSingle(), r.ReadSingle(), r.ReadSingle());
    private static void Write(BinaryWriter w, CharacterAirConfig c) { w.Write(c.MaximumSpeed); w.Write(c.Acceleration); w.Write(c.Braking); w.Write(c.WishSpeedCap); w.Write(c.LateralControl); w.Write(c.Drag); }
    private static CharacterAirConfig ReadAir(BinaryReader r) => new(r.ReadSingle(), r.ReadSingle(), r.ReadSingle(), r.ReadSingle(), r.ReadSingle(), r.ReadSingle());
    private static void Write(BinaryWriter w, CharacterVerticalConfig c) { w.Write(c.Gravity); w.Write(c.TerminalRiseSpeed); w.Write(c.TerminalFallSpeed); w.Write(c.JumpSpeed); w.Write(c.GroundedDownwardBias); }
    private static CharacterVerticalConfig ReadVertical(BinaryReader r) => new(r.ReadSingle(), r.ReadSingle(), r.ReadSingle(), r.ReadSingle(), r.ReadSingle());
    private static void Write(BinaryWriter w, CharacterJumpConfig c) { w.Write(c.BufferSeconds); w.Write(c.CoyoteSeconds); w.Write(c.LandingLockoutSeconds); w.Write(c.HeldInputRetriggers); }
    private static CharacterJumpConfig ReadJump(BinaryReader r) => new(r.ReadSingle(), r.ReadSingle(), r.ReadSingle(), r.ReadBoolean());
    private static void Write(BinaryWriter w, CharacterSurfaceConfig c) { w.Write(c.MaximumSlopeRadians); w.Write(c.SlopeHysteresisRadians); w.Write(c.SteepSlideAcceleration); w.Write(c.SteepSlideSpeed); w.Write(c.MaximumStepHeight); w.Write(c.MinimumStepWidth); w.Write(c.FloorSnapDistance); w.Write(c.FloorSnapSpeedLimit); w.Write(c.LedgeSupportFraction); }
    private static CharacterSurfaceConfig ReadSurface(BinaryReader r) => new(r.ReadSingle(), r.ReadSingle(), r.ReadSingle(), r.ReadSingle(), r.ReadSingle(), r.ReadSingle(), r.ReadSingle(), r.ReadSingle(), r.ReadSingle());
    private static void Write(BinaryWriter w, CharacterRecoveryConfig c) { w.Write(c.MaximumDistance); w.Write(c.MaximumSpeed); w.Write(c.NormalNudge); w.Write(c.UnresolvedTolerance); }
    private static CharacterRecoveryConfig ReadRecovery(BinaryReader r) => new(r.ReadSingle(), r.ReadSingle(), r.ReadSingle(), r.ReadSingle());
    private static void Write(BinaryWriter w, CharacterPlatformConfig c) { w.Write(c.CarryTranslation); w.Write(c.CarryRotation); w.Write(c.InheritDepartureVelocity); w.Write(c.DepartureVelocityFactor); w.Write(c.SupportLossGraceSeconds); w.Write(c.CrushTolerance); }
    private static CharacterPlatformConfig ReadPlatform(BinaryReader r) => new(r.ReadBoolean(), r.ReadBoolean(), r.ReadBoolean(), r.ReadSingle(), r.ReadSingle(), r.ReadSingle());
    private static void Write(BinaryWriter w, CharacterExternalMotionConfig c) { w.Write(c.ImpulseScale); w.Write(c.ExternalDecayPerSecond); w.Write(c.MaximumExternalSpeed); w.Write(c.AuthoredMass); w.Write(c.DynamicImpulseFactor); w.Write(c.MaximumDynamicImpulse); }
    private static CharacterExternalMotionConfig ReadExternal(BinaryReader r) => new(r.ReadSingle(), r.ReadSingle(), r.ReadSingle(), r.ReadSingle(), r.ReadSingle(), r.ReadSingle());
    private static void Write(BinaryWriter w, CharacterSolverConfig c) { w.Write(c.MaximumSlidePlanes); w.Write(c.MaximumCastIterations); w.Write(c.MaximumRecoveryPasses); w.Write(c.MaximumContacts); w.Write(c.MaximumStepAttempts); w.Write(c.MaximumDisplacementPerStep); w.Write(c.MaximumQueriesPerStep); }
    private static CharacterSolverConfig ReadSolver(BinaryReader r) => new(r.ReadUInt32(), r.ReadUInt32(), r.ReadUInt32(), r.ReadUInt32(), r.ReadUInt32(), r.ReadSingle(), r.ReadUInt32());
}
