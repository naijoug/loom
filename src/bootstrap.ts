// This tiny entry loads before the React graph, so module/init failures cannot leave an empty root.
import { prepareStartup, startupFailed } from "./appStartup";
const retry = document.getElementById("loom-startup-retry");
retry?.addEventListener("click", () => window.location.reload());
void prepareStartup().then(() => import("./main")).then(module => module.render()).catch(startupFailed);
