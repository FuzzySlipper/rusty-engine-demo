import type { Routes } from "@angular/router";
import { DiagnosticsScreenComponent } from "./diagnostics-screen.component";
import { GameScreenComponent } from "./game-screen.component";

export const appRoutes: Routes = [
  { path: "", component: GameScreenComponent, title: "Loading Bay" },
  {
    path: "diagnostics",
    component: DiagnosticsScreenComponent,
    title: "Loading Bay diagnostics",
  },
  { path: "**", redirectTo: "" },
];
