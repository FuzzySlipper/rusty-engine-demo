# Presentation frame

The Product Browser Host owns one Engine canvas. Angular mounts its HUD and failure/loading presentation in the same bounded product frame; it never creates, sizes, or replaces the Engine canvas.

Semantic input is accepted only inside that frame. Pointer-look is bounded before it reaches the Engine host, and outside gutters must not move, look, fire, or select a product action. Loading, runtime failure, and HUD readouts remain visible within the frame so a focused browser capture shows the relevant product state without a hidden pane or second window.

This document describes the supported browser presentation frame.
