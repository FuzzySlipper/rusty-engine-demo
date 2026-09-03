using Rusty.Engine;

namespace LoadingBay.Game;

/// <summary>
/// Keeps Loading Bay's authored sky in the Engine camera-view service for the
/// lifetime of the product, independently of individual gameplay sessions.
/// </summary>
internal sealed class LoadingBaySkyBackground : IDisposable
{
    internal const string E1M1SkySourcePath = "doom-e1m1/textures/sky/SKY1.png";
    private const ulong E1M1SkyByteLength = 16636;
    private static readonly ContentSha256 E1M1SkySha256 = new(
        0x2834a39fac8f538bUL, 0xbc340ab8a7d78025UL, 0x1669f81d43ef1e94UL, 0xa0db551fe8c66429UL);

    private readonly ContentReference _content;
    private readonly ICameraViewService _cameraView;
    private readonly LoadingBaySkyReadout _readout;
    private bool _disposed;

    internal LoadingBaySkyBackground(
        IContentService content,
        ProductContent admitted,
        IAppearanceService appearance,
        ICameraViewService cameraView)
    {
        ArgumentNullException.ThrowIfNull(content);
        ArgumentNullException.ThrowIfNull(admitted);
        ArgumentNullException.ThrowIfNull(appearance);
        _cameraView = cameraView ?? throw new ArgumentNullException(nameof(cameraView));

        LoadingBayAdmittedContent.RequireAdmitted(admitted, E1M1SkySourcePath);
        ContentReference? skyContent = null;
        try
        {
            skyContent = content.OpenReference(new ContentOpenRequest(E1M1SkySourcePath));
            ContentReferenceInfo skyInfo = LoadingBayAdmittedContent.RequireExact(
                content.ReadReferenceInfo(skyContent), E1M1SkySourcePath, E1M1SkySha256);
            if (skyInfo.ByteLength != E1M1SkyByteLength)
                throw new InvalidOperationException($"Loading Bay's retained SKY1 provenance length changed: {skyInfo.ByteLength}.");

            // Appearance accepts both content-relative and content-prefixed paths. Keep the
            // canonical product identity relative while making the renderer resource path
            // match the authored catalog's sourcePath exactly.
            RenderResourceInfo sky = appearance.OpenResource(new RenderResourceRequest($"content/{E1M1SkySourcePath}"));
            if (sky.Kind != RenderResourceKind.Texture || sky.ByteLength != E1M1SkyByteLength || sky.Handle.Value == 0)
                throw new InvalidOperationException("Engine did not admit Loading Bay's E1M1 sky as the retained SKY1 texture.");
            _cameraView.SetSkyBackground(sky.Handle);
            _content = skyContent;
            _readout = new LoadingBaySkyReadout(E1M1SkySourcePath, E1M1SkySha256, skyInfo.ByteLength, sky.Handle.Value, true, true);
        }
        catch
        {
            skyContent?.Dispose();
            throw;
        }
    }

    internal LoadingBaySkyReadout Readout => _readout;

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        List<Exception>? failures = null;
        try { _cameraView.ClearSkyBackground(new ClearSkyBackgroundRequest()); }
        catch (Exception exception) { failures = [exception]; }
        try { _content.Dispose(); }
        catch (Exception exception) { (failures ??= []).Add(exception); }
        if (failures is { Count: > 0 }) throw new AggregateException(failures);
    }
}
