using Rusty.Engine;

namespace LoadingBay.Game;

/// <summary>
/// Retains the authored E1M1 material closure and its Engine-owned voxel-scene projection.
/// </summary>
internal sealed class LoadingBayVoxelScenePresentation : IDisposable
{
    private static readonly ContentSha256 AssetCatalogSha256 = new(
        0x3a5e5347b12e5225UL, 0x38950ca085e544f0UL, 0x0be63b50bff42486UL, 0x0cde01b088f25643UL);

    private readonly ContentReference _catalogContent;
    private readonly AuthoredCatalog _catalog;
    private readonly List<Material> _materials;
    private readonly IVoxelScenePresentationService _service;
    private readonly VoxelScenePresentation _presentation;
    private readonly Dictionary<uint, ulong> _materialHandlesBySlot;
    private readonly LoadingBayVoxelSceneReadout _identity;
    private LoadingBayVoxelSceneReadout _readout;
    private readonly int _materialCount;
    private bool _disposed;

    internal LoadingBayVoxelScenePresentation(
        IEngineContext engine,
        ProductContent admitted,
        SpatialSession session,
        VoxelAssetSpatialPublishLeaseReceipt publishedScene)
    {
        ArgumentNullException.ThrowIfNull(engine);
        ArgumentNullException.ThrowIfNull(admitted);
        ArgumentNullException.ThrowIfNull(session);
        LoadingBayAdmittedContent.RequireAdmitted(admitted, LoadingBayAdmittedContent.AssetCatalogPath);

        ContentReference? catalogContent = null;
        AuthoredCatalog? catalog = null;
        List<Material> materials = [];
        List<LoadingBayAuthoredMaterialReadout> materialReadouts = [];
        Dictionary<uint, ulong> materialHandlesBySlot = [];
        VoxelScenePresentation? presentation = null;
        try
        {
            catalogContent = engine.Content.OpenReference(new ContentOpenRequest(LoadingBayAdmittedContent.AssetCatalogPath));
            LoadingBayAdmittedContent.RequireExact(
                engine.Content.ReadReferenceInfo(catalogContent),
                LoadingBayAdmittedContent.AssetCatalogPath,
                AssetCatalogSha256);
            catalog = engine.AuthoredContent.AdmitCatalogFromContent(new AuthoredCatalogFromContentRequest(catalogContent));
            AuthoredCatalogReadoutLeaseReceipt catalogReadout = engine.AuthoredContent.ReadCatalog(catalog);
            ValidateCatalog(catalogReadout);

            Dictionary<string, VoxelAssetSpatialPaletteRow> palette = IndexPalette(publishedScene);
            List<VoxelSceneMaterialBinding> bindings = new(catalogReadout.Materials.Length);
            HashSet<string> catalogMaterials = new(StringComparer.Ordinal);
            foreach (AuthoredMaterialReadout catalogMaterial in catalogReadout.Materials.Span)
            {
                if (string.IsNullOrWhiteSpace(catalogMaterial.EntryId) || !catalogMaterials.Add(catalogMaterial.EntryId))
                    throw new InvalidOperationException("The E1M1 authored catalog contains a duplicate or incomplete material identity.");
                if (!palette.TryGetValue(catalogMaterial.EntryId, out VoxelAssetSpatialPaletteRow paletteRow))
                    throw new InvalidOperationException($"The E1M1 voxel palette does not contain authored material '{catalogMaterial.EntryId}'.");
                AuthoredCatalogEntryReadout materialEntry = RequireExactlyOneEntry(catalogReadout.Entries, catalogMaterial.EntryId, AssetKind.Material);
                if (!materialEntry.HasHash || string.IsNullOrWhiteSpace(materialEntry.Hash))
                    throw new InvalidOperationException($"Authored material '{catalogMaterial.EntryId}' has no retained catalog provenance hash.");

                // The catalog and stable material identity are product facts. Engine resolves the
                // retained authored material, texture identity, and canonical voxel-surface
                // descriptor as one public operation, rather than asking Loading Bay to recreate
                // those renderer details from readouts.
                AuthoredMaterialResolutionLeaseReceipt resolvedMaterial = engine.AuthoredContent.ResolveMaterial(
                    new AuthoredMaterialResolveRequest(catalog, catalogMaterial.EntryId));
                AuthoredMaterialReadout material = RequireExactlyOneMaterial(resolvedMaterial.Materials, catalogMaterial.EntryId);
                if (!material.HasVoxelSurface)
                    throw new InvalidOperationException($"Authored material '{catalogMaterial.EntryId}' has no retained voxel-surface descriptor.");
                AuthoredVoxelSurfaceReadout surface = RequireExactlyOneSurface(resolvedMaterial.VoxelSurfaces, catalogMaterial.EntryId);
                if (!surface.HasResolvedMapping || string.IsNullOrWhiteSpace(surface.ResolvedTexture.Id))
                    throw new InvalidOperationException($"Authored material '{catalogMaterial.EntryId}' has no resolved E1M1 texture mapping.");

                AuthoredAssetReference texture = surface.ResolvedTexture;
                AuthoredResolvedEntryLeaseReceipt resolvedTexture = engine.AuthoredContent.ResolveReference(
                    new AuthoredCatalogResolveRequest(
                        catalog,
                        texture.Id,
                        texture.VersionKind,
                        texture.Version,
                        texture.HasHash,
                        texture.Hash,
                        true,
                        AuthoredFallbackContext.CosmeticSurface));
                AuthoredCatalogEntryReadout textureEntry = RequireExactlyOneTextureEntry(resolvedTexture.Entry, texture.Id);
                if (!textureEntry.HasHash || string.IsNullOrWhiteSpace(textureEntry.Hash) || !textureEntry.HasSourcePath || string.IsNullOrWhiteSpace(textureEntry.SourcePath))
                    throw new InvalidOperationException($"Authored texture '{texture.Id}' has no Engine catalog source path.");
                if (!surface.ResolvedTexture.HasHash || surface.ResolvedTexture.Hash != textureEntry.Hash)
                    throw new InvalidOperationException($"Authored texture '{texture.Id}' lost its retained provenance hash during resolution.");

                RenderResourceInfo resource = engine.Appearance.OpenResource(new RenderResourceRequest(textureEntry.SourcePath));
                if (resource.Kind != RenderResourceKind.Texture || resource.ByteLength == 0 || resource.Handle.Value == 0)
                    throw new InvalidOperationException($"Engine could not open authored texture '{texture.Id}' as a usable texture resource.");
                Material appearanceMaterial = engine.Appearance.CreateAuthoredMaterial(
                    new AuthoredMaterialAppearanceRequest(catalog, catalogMaterial.EntryId, resource.Handle));
                materials.Add(appearanceMaterial);
                bindings.Add(new VoxelSceneMaterialBinding(paletteRow.MaterialSlot, appearanceMaterial));
                if (!materialHandlesBySlot.TryAdd(paletteRow.MaterialSlot, appearanceMaterial.Handle.Value))
                    throw new InvalidOperationException($"The E1M1 voxel palette contains duplicate material slot {paletteRow.MaterialSlot}.");
                materialReadouts.Add(new LoadingBayAuthoredMaterialReadout(
                    catalogMaterial.EntryId,
                    materialEntry.Version,
                    materialEntry.Hash,
                    paletteRow.MaterialSlot,
                    appearanceMaterial.Handle.Value,
                    texture.Id,
                    textureEntry.Version,
                    textureEntry.Hash,
                    textureEntry.SourcePath,
                    material.UvStrategy,
                    surface.MappingKind,
                    surface.TileScaleX,
                    surface.TileScaleY,
                    surface.TileOriginX,
                    surface.TileOriginY,
                    surface.Atlas.Id,
                    surface.Region,
                    surface.ResolvedFilter,
                    surface.ResolvedWrap));
            }

            if (catalogMaterials.Count != catalogReadout.Materials.Length || bindings.Count != catalogReadout.Materials.Length)
                throw new InvalidOperationException("The E1M1 authored presentation closure did not produce one binding for every catalog material.");
            presentation = engine.VoxelScenePresentation.ProjectScene(new ProjectVoxelSceneRequest(session, bindings.ToArray()));
            VoxelScenePresentationReadout presentationReadout = ValidatePresentation(
                engine.VoxelScenePresentation.RefreshScene(presentation), bindings.Count);
            VoxelSceneMaterialMappingLeaseReceipt mappingReadout = engine.VoxelScenePresentation.ReadMaterialMapping(presentation);
            ValidateMaterialMapping(mappingReadout, materialHandlesBySlot);

            _catalogContent = catalogContent;
            _catalog = catalog;
            _materials = materials;
            _service = engine.VoxelScenePresentation;
            _presentation = presentation;
            _materialHandlesBySlot = materialHandlesBySlot;
            _identity = new LoadingBayVoxelSceneReadout(
                LoadingBayAdmittedContent.AssetCatalogPath,
                catalogReadout.CanonicalHash,
                catalogReadout.EntryCount,
                checked((uint)materialReadouts.Count),
                checked((uint)bindings.Count),
                checked((uint)mappingReadout.Mappings.Length),
                mappingReadout.SourceRevision,
                mappingReadout.MeshRevision,
                true,
                presentationReadout.SourceRevision,
                presentationReadout.MeshRevision,
                presentationReadout.ChunkCount,
                materialReadouts.ToArray());
            _readout = _identity;
            _materialCount = bindings.Count;
        }
        catch
        {
            presentation?.Dispose();
            for (int index = materials.Count - 1; index >= 0; index--) materials[index].Dispose();
            catalog?.Dispose();
            catalogContent?.Dispose();
            throw;
        }
    }

    internal void Refresh()
    {
        if (_disposed) throw new ObjectDisposedException(nameof(LoadingBayVoxelScenePresentation));
        VoxelScenePresentationReadout presentation = ValidatePresentation(_service.RefreshScene(_presentation), _materialCount);
        VoxelSceneMaterialMappingLeaseReceipt mapping = _service.ReadMaterialMapping(_presentation);
        ValidateMaterialMapping(mapping, _materialHandlesBySlot);
        _readout = _identity with
        {
            MappingCount = checked((uint)mapping.Mappings.Length),
            MappingSourceRevision = mapping.SourceRevision,
            MappingMeshRevision = mapping.MeshRevision,
            SceneSourceRevision = presentation.SourceRevision,
            SceneMeshRevision = presentation.MeshRevision,
            SceneChunkCount = presentation.ChunkCount,
            Realized = presentation.Present,
        };
    }

    internal LoadingBayVoxelSceneReadout Readout => _readout;

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        List<Exception>? failures = null;
        try { _presentation.Dispose(); }
        catch (Exception exception) { failures = [exception]; }
        for (int index = _materials.Count - 1; index >= 0; index--)
        {
            try { _materials[index].Dispose(); }
            catch (Exception exception) { (failures ??= []).Add(exception); }
        }
        try { _catalog.Dispose(); }
        catch (Exception exception) { (failures ??= []).Add(exception); }
        try { _catalogContent.Dispose(); }
        catch (Exception exception) { (failures ??= []).Add(exception); }
        if (failures is { Count: > 0 }) throw new AggregateException(failures);
    }

    private static void ValidateCatalog(AuthoredCatalogReadoutLeaseReceipt catalog)
    {
        if (catalog.Materials.Length == 0 || catalog.Textures.Length != catalog.Materials.Length ||
            catalog.VoxelSurfaces.Length != catalog.Materials.Length || catalog.Entries.Length != catalog.Materials.Length + catalog.Textures.Length ||
            catalog.EntryCount != catalog.Entries.Length ||
            string.IsNullOrWhiteSpace(catalog.CanonicalHash))
            throw new InvalidOperationException("Engine did not retain a complete E1M1 authored presentation closure.");
    }

    private static Dictionary<string, VoxelAssetSpatialPaletteRow> IndexPalette(VoxelAssetSpatialPublishLeaseReceipt receipt)
    {
        Dictionary<string, VoxelAssetSpatialPaletteRow> palette = new(StringComparer.Ordinal);
        foreach (VoxelAssetSpatialPaletteRow row in receipt.Palette.Span)
            if (row.MaterialSlot == 0 || string.IsNullOrWhiteSpace(row.MaterialAssetId) || !palette.TryAdd(row.MaterialAssetId, row))
                throw new InvalidOperationException("Engine voxel publication returned duplicate or incomplete E1M1 palette material identities.");
        return palette;
    }

    private static AuthoredVoxelSurfaceReadout RequireExactlyOneSurface(ReadOnlyMemory<AuthoredVoxelSurfaceReadout> values, string materialId)
    {
        if (values.Length != 1 || values.Span[0].MaterialEntryId != materialId)
            throw new InvalidOperationException($"Engine AuthoredContent did not resolve one voxel surface for E1M1 material '{materialId}'.");
        return values.Span[0];
    }

    private static AuthoredCatalogEntryReadout RequireExactlyOneTextureEntry(ReadOnlyMemory<AuthoredCatalogEntryReadout> values, string textureId)
    {
        if (values.Length != 1 || values.Span[0].Id != textureId || values.Span[0].Kind != AssetKind.Texture)
            throw new InvalidOperationException($"Engine AuthoredContent did not resolve E1M1 texture '{textureId}' exactly once.");
        return values.Span[0];
    }

    private static AuthoredCatalogEntryReadout RequireExactlyOneEntry(ReadOnlyMemory<AuthoredCatalogEntryReadout> values, string entryId, AssetKind kind)
    {
        AuthoredCatalogEntryReadout[] matches = values.Span.ToArray().Where(value => value.Id == entryId && value.Kind == kind).ToArray();
        if (matches.Length != 1)
            throw new InvalidOperationException($"Engine AuthoredContent did not retain authored {kind} '{entryId}' exactly once.");
        return matches[0];
    }

    private static AuthoredMaterialReadout RequireExactlyOneMaterial(ReadOnlyMemory<AuthoredMaterialReadout> values, string materialId)
    {
        if (values.Length != 1 || values.Span[0].EntryId != materialId)
            throw new InvalidOperationException($"Engine AuthoredContent did not resolve E1M1 material '{materialId}' exactly once.");
        return values.Span[0];
    }

    private static VoxelScenePresentationReadout ValidatePresentation(VoxelScenePresentationReadout readout, int materialCount)
    {
        if (!readout.Present || readout.ChunkCount == 0 || readout.MaterialCount != materialCount)
            throw new InvalidOperationException("Engine did not project the complete textured E1M1 voxel scene.");
        return readout;
    }

    private static void ValidateMaterialMapping(VoxelSceneMaterialMappingLeaseReceipt mapping, IReadOnlyDictionary<uint, ulong> materialHandlesBySlot)
    {
        if (mapping.Mappings.Length == 0)
            throw new InvalidOperationException("Engine did not retain an effective E1M1 voxel material mapping.");
        HashSet<(uint SourceSlot, SpatialFace Face)> seen = [];
        HashSet<uint> mappedSlots = [];
        foreach (VoxelSceneMaterialMappingRow row in mapping.Mappings.Span)
        {
            if (!materialHandlesBySlot.TryGetValue(row.SourceSlot, out ulong materialHandle) ||
                row.MaterialValue != materialHandle || row.RendererSlot == 0 || !seen.Add((row.SourceSlot, row.Face)))
                throw new InvalidOperationException("Engine voxel material mapping lost canonical E1M1 material provenance.");
            mappedSlots.Add(row.SourceSlot);
        }
        if (!mappedSlots.SetEquals(materialHandlesBySlot.Keys))
            throw new InvalidOperationException("Engine voxel material mapping did not realize every canonical E1M1 material slot.");
    }
}
