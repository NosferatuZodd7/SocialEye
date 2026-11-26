import { HashRouter, Route, Routes } from "react-router-dom";
import { Layout } from "./components/Layout";
import { FeedbackProvider } from "./components/feedback";
import { LangContext } from "./lib/i18n";
import { StoreProvider, useStore } from "./lib/store";
import { Spinner } from "./components/ui";
import Dashboard from "./pages/Dashboard";
import Accounts from "./pages/Accounts";
import PlatformPage from "./pages/PlatformPage";
import Studio from "./pages/Studio";
import CalendarPage from "./pages/Calendar";
import Analytics from "./pages/Analytics";
import Automations from "./pages/Automations";
import Notifications from "./pages/Notifications";
import Reports from "./pages/Reports";
import SettingsPage from "./pages/Settings";

function Shell() {
  const { lang, ready } = useStore();
  return (
    <LangContext.Provider value={lang}>
      <FeedbackProvider>
        {!ready ? <div className="p-8"><Spinner /></div> : (
          <HashRouter>
            <Routes>
              <Route element={<Layout />}>
                <Route index element={<Dashboard />} />
                <Route path="accounts" element={<Accounts />} />
                <Route path="instagram" element={<PlatformPage platform="instagram" />} />
                <Route path="facebook" element={<PlatformPage platform="facebook" />} />
                <Route path="youtube" element={<PlatformPage platform="youtube" />} />
                <Route path="spotify" element={<PlatformPage platform="spotify" />} />
                <Route path="studio" element={<Studio />} />
                <Route path="calendar" element={<CalendarPage />} />
                <Route path="analytics" element={<Analytics />} />
                <Route path="automations" element={<Automations />} />
                <Route path="notifications" element={<Notifications />} />
                <Route path="reports" element={<Reports />} />
                <Route path="settings" element={<SettingsPage />} />
              </Route>
            </Routes>
          </HashRouter>
        )}
      </FeedbackProvider>
    </LangContext.Provider>
  );
}

export default function App() {
  return <StoreProvider><Shell /></StoreProvider>;
}
