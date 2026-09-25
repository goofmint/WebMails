import { Sidebar } from "./components/Sidebar";
import { ShellStoreProvider } from "./store/ShellStoreProvider";
import { shellIpc } from "./ipc";

// The settings screen itself is Task 1.12's `#/settings` route; this shell
// webview only needs to recognize that route and render nothing for it —
// the settings window loads the same bundle, but at its own `index.html`
// entry with a hash Rust already sets in commands/mod.rs's
// `SETTINGS_WINDOW_PATH`.
const SETTINGS_ROUTE_PREFIX = "#/settings";

function App() {
  if (window.location.hash.startsWith(SETTINGS_ROUTE_PREFIX)) {
    return null;
  }

  return (
    <ShellStoreProvider ipc={shellIpc}>
      <Sidebar />
    </ShellStoreProvider>
  );
}

export default App;
