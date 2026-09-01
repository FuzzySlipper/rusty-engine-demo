using Rusty.Engine.Entities;

namespace LoadingBay.Game;

/// <summary>Optional lifecycle seam for exposing the session's current Engine entity projection.</summary>
internal interface ILoadingBayDebugSession
{
    EntityWorld DebugEntityWorld { get; }

    /// <summary>Receives a replacement whenever persistence installs a fresh Engine projection.</summary>
    void SetDebugEntityWorldChanged(Action<EntityWorld>? callback);
}
