import type { Routes } from "@angular/router";
import { DiagnosticsScreenComponent } from "./diagnostics-screen.component";
import { GameScreenComponent } from "./game-screen.component";
import { MainMenuComponent } from "./main-menu.component";
import { SettingsScreenComponent } from "./settings-screen.component";

export const appRoutes: Routes = [
  { path: "", component: MainMenuComponent, title: "Loading Bay" },
  { path: "game", component: GameScreenComponent, title: "Loading Bay" },
  {
    path: "settings",
    component: SettingsScreenComponent,
    title: "Loading Bay settings",
  },
  {
    path: "diagnostics",
    component: DiagnosticsScreenComponent,
    title: "Loading Bay diagnostics",
  },
  { path: "**", redirectTo: "" },
];
