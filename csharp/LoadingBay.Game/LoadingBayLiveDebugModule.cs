using Rusty.Engine.Debugging;

namespace LoadingBay.Game;

/// <summary>Explicit, bounded product debug operations for the current live session.</summary>
public sealed class LoadingBayLiveDebugModule : IDebugCommandModule
{
    private readonly Func<string> _readout;
    private readonly Func<string, int, DebugCommandResult> _setTrack;

    public LoadingBayLiveDebugModule(Func<string> readout, Func<string, int, DebugCommandResult> setTrack)
    {
        _readout = readout ?? throw new ArgumentNullException(nameof(readout));
        _setTrack = setTrack ?? throw new ArgumentNullException(nameof(setTrack));
    }

    [DebugCommand("loading-bay.readout", Description = "Shows the current bounded Loading Bay session readout.")]
    public string Readout() => _readout();

    [DebugCommand("loading-bay.set-track", Description = "Sets the current health or armor track within its authored bounds.")]
    public DebugCommandResult SetTrack(string track, int value) => _setTrack(track, value);
}
