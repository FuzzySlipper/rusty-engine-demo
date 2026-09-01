using Rusty.Engine;

namespace LoadingBay.Game;

/// <summary>
/// Keeps Loading Bay's authored sky in the Engine camera-view service for the
/// lifetime of the product, independently of individual gameplay sessions.
/// </summary>
internal sealed class LoadingBaySkyBackground : IDisposable
{
    private const string E1M1SkySourcePath = "doom-e1m1/textures/sky/SKY1.png";

    private readonly ICameraViewService _cameraView;
    private bool _disposed;

    internal LoadingBaySkyBackground(IAppearanceService appearance, ICameraViewService cameraView)
    {
        ArgumentNullException.ThrowIfNull(appearance);
        _cameraView = cameraView ?? throw new ArgumentNullException(nameof(cameraView));

        RenderResourceInfo sky = appearance.OpenResource(new RenderResourceRequest(E1M1SkySourcePath));
        if (sky.Kind != RenderResourceKind.Texture || sky.ByteLength == 0 || sky.Handle.Value == 0)
            throw new InvalidOperationException("Engine did not admit Loading Bay's E1M1 sky as a usable texture resource.");
        _cameraView.SetSkyBackground(sky.Handle);
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        _cameraView.ClearSkyBackground(new ClearSkyBackgroundRequest());
    }
}
