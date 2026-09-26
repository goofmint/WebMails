import { Sidebar } from "./components/Sidebar";
import { ShellStoreProvider } from "./store/ShellStoreProvider";
import { shellIpc, settingsIpc } from "./ipc";
import { SettingsApp } from "./settings/SettingsApp";
import { isSettingsRoute } from "./settings/route";

// The settings window loads the same bundle as the shell, but at its own
// `index.html` entry with a hash Rust already sets in commands/mod.rs's
// `SETTINGS_WINDOW_PATH` — `isSettingsRoute` (Task 1.12) is the pure
// function that recognizes it.
function App() {
  if (isSettingsRoute(window.location.hash)) {
    return <SettingsApp ipc={settingsIpc} />;
  }

  return (
    <ShellStoreProvider ipc={shellIpc}>
      <Sidebar />
    </ShellStoreProvider>
  );
}

export default App;
