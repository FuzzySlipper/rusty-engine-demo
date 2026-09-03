using Rusty.Engine;

namespace LoadingBay.Game;

/// <summary>Internal seam for the product-owned session and direct lifecycle exercise.</summary>
internal interface ILoadingBaySession : IDisposable
{
    ProductUpdateResult Update(ProductUpdate update);

    void Publish();

    /// <summary>Republishes retained Engine realizations for a fresh renderer attachment.</summary>
    void Attach() => Publish();

    /// <summary>Enables this generation to publish the product-owned E1M1 realization handles.</summary>
    void ActivateSharedRealizations();

    /// <summary>Returns a failed replacement to preflight-only publication.</summary>
    void DeactivateSharedRealizations();

    LoadingBayReadout Readout();

    /// <summary>Returns copied Engine material/sky realization diagnostics for live debugging.</summary>
    LoadingBayEngineServiceReadout EngineReadout() => LoadingBayEngineServiceReadout.Empty;

    LoadingBayReceipt DeveloperSetTrack(ulong generation, string track, int value, string correlation);

}
