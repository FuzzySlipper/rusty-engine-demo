using Mechanics = Rusty.Engine.Mechanics;

namespace LoadingBay.Game;

internal static class LoadingBayDefinitions
{
    internal static readonly LoadingBayItem Bullets = Ammunition("ammo/bullets");
    internal static readonly LoadingBayItem Shells = Ammunition("ammo/shells");
    internal static readonly LoadingBayItem HealthBonus = HealthSupply("supply/health-bonus");
    internal static readonly LoadingBayItem Medikit = HealthSupply("supply/medikit");
    internal static readonly LoadingBayItem Stimpack = HealthSupply("supply/stimpack");
    internal static readonly LoadingBayItem BlueArmor = Armor("armor/blue");
    internal static readonly LoadingBayItem GreenArmor = Armor("armor/green");
    internal static readonly LoadingBayItem ArmorBonus = Armor("armor/bonus");
    internal static readonly LoadingBayWeapon Fist = Weapon("weapon/fist");
    internal static readonly LoadingBayWeapon Pistol = Weapon("weapon/pistol");
    internal static readonly LoadingBayWeapon Shotgun = Weapon("weapon/shotgun");
    internal static readonly Mechanics.EquipmentSlotDefinition WeaponSlot = new(
        Mechanics.EquipmentSlotId.Parse("loading-bay.weapon"),
        [Mechanics.ItemClassificationId.Parse("loading-bay.weapon")]);

    internal static readonly IReadOnlyDictionary<string, LoadingBayWeapon> Weapons =
        new Dictionary<string, LoadingBayWeapon>(StringComparer.Ordinal)
        {
            [Fist.Id] = Fist,
            [Pistol.Id] = Pistol,
            [Shotgun.Id] = Shotgun,
        };

    internal static LoadingBayItem Item(string id) => id switch
    {
        "ammo/bullets" => Bullets,
        "ammo/shells" => Shells,
        "supply/health-bonus" => HealthBonus,
        "supply/medikit" => Medikit,
        "supply/stimpack" => Stimpack,
        "armor/blue" => BlueArmor,
        "armor/green" => GreenArmor,
        "armor/bonus" => ArmorBonus,
        _ => throw new ArgumentOutOfRangeException(nameof(id)),
    };
    private static LoadingBayItem Item(string id, LoadingBayItemKind kind, ulong maximum, LoadingBayPickupPolicy? policy = null) => new(id, kind, new Mechanics.ItemDefinition(Mechanics.ItemDefinitionId.Parse("loading-bay." + id.Replace('/', '.')), Mechanics.ItemKind.Fungible, maximum), policy);
    private static LoadingBayItem Ammunition(string id)
    {
        LoadingBayE1M1Ammunition semantic = LoadingBayE1M1SemanticCatalog.Item<LoadingBayE1M1Ammunition>(id);
        return Item(id, LoadingBayItemKind.Ammunition, semantic.MaximumQuantity);
    }
    private static LoadingBayItem HealthSupply(string id)
    {
        LoadingBayE1M1HealthSupply semantic = LoadingBayE1M1SemanticCatalog.Item<LoadingBayE1M1HealthSupply>(id);
        if (!semantic.AutomaticUse) throw new InvalidOperationException($"E1M1 health supply '{id}' is not automatic.");
        return Item(id, LoadingBayItemKind.Health, 1, new LoadingBayPickupPolicy.Restore(semantic.RestoreHealth, semantic.MaximumHealth, semantic.ConsumeAtCap));
    }
    private static LoadingBayItem Armor(string id)
    {
        LoadingBayE1M1Armor semantic = LoadingBayE1M1SemanticCatalog.Item<LoadingBayE1M1Armor>(id);
        LoadingBayPickupPolicy policy = semantic.GrantMode switch
        {
            LoadingBayE1M1ArmorGrantMode.SetMinimum => new LoadingBayPickupPolicy.SetMinimum(semantic.Protection, ArmorProtection(id, semantic.AbsorptionDivisor)),
            LoadingBayE1M1ArmorGrantMode.None when semantic.Transition == LoadingBayE1M1ArmorTransition.Preserve => new LoadingBayPickupPolicy.RestoreArmor(semantic.Protection, semantic.MaximumArmor, semantic.ConsumeAtCap, ArmorProtection(id, semantic.AbsorptionDivisor)),
            _ => throw new InvalidOperationException($"Unsupported E1M1 armor policy '{id}'."),
        };
        return Item(id, LoadingBayItemKind.Armor, semantic.MaximumQuantity, policy);
    }
    internal static bool IsKnownArmorProtection(LoadingBayArmorProtection protection) => protection == LoadingBayArmorProtection.None
        || protection == ArmorProtection("armor/blue", LoadingBayE1M1SemanticCatalog.Item<LoadingBayE1M1Armor>("armor/blue").AbsorptionDivisor)
        || protection == ArmorProtection("armor/green", LoadingBayE1M1SemanticCatalog.Item<LoadingBayE1M1Armor>("armor/green").AbsorptionDivisor)
        || protection == ArmorProtection("armor/bonus", LoadingBayE1M1SemanticCatalog.Item<LoadingBayE1M1Armor>("armor/bonus").AbsorptionDivisor);
    private static LoadingBayArmorProtection ArmorProtection(string id, int divisor) => id switch
    {
        "armor/blue" => new LoadingBayArmorProtection(LoadingBayArmorProtectionMode.Blue, divisor),
        "armor/green" => new LoadingBayArmorProtection(LoadingBayArmorProtectionMode.Green, divisor),
        "armor/bonus" => new LoadingBayArmorProtection(LoadingBayArmorProtectionMode.Bonus, divisor),
        _ => throw new ArgumentOutOfRangeException(nameof(id)),
    };
    private static LoadingBayWeapon Weapon(string id)
    {
        LoadingBayE1M1Weapon semantic = LoadingBayE1M1SemanticCatalog.Item<LoadingBayE1M1Weapon>(id);
        return new(id, new Mechanics.ItemDefinition(
        Mechanics.ItemDefinitionId.Parse("loading-bay." + id.Replace('/', '.')),
        Mechanics.ItemKind.Unique,
        maximumQuantity: semantic.MaximumQuantity,
        classifications: [Mechanics.ItemClassificationId.Parse("loading-bay.weapon")],
        equipment: new Mechanics.ItemEquipmentPolicy(requiredSlots: 1)));
    }
}
