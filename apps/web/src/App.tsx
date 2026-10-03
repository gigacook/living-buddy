import { lazy, Suspense, useEffect, useRef } from "react";
import { NavLink, Route, Routes, useLocation } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Bell, CalendarDays, CheckSquare, Inbox, Settings as SettingsIcon, Sun, Timer, Users } from "lucide-react";
import { api } from "./lib/api";
import { keys, useMe, useMembers, useSession } from "./lib/queries";
import { useActorId } from "./lib/prefs";
import { Mascot } from "./components/Mascot";
import { AlarmWatcher, TimerWatcher } from "./components/Watchers";
import { ErrorState, Loading } from "./components/ui";
import { Onboarding } from "./pages/Onboarding";
import { PairDevice } from "./pages/PairDevice";
import { Today } from "./pages/Today";

const Tasks = lazy(() => import("./pages/Tasks"));
const Focus = lazy(() => import("./pages/Focus"));
const Calendar = lazy(() => import("./pages/Calendar"));
const Groups = lazy(() => import("./pages/Groups"));
const GroupDetail = lazy(() => import("./pages/GroupDetail"));
const InboxPage = lazy(() => import("./pages/Inbox"));
const Settings = lazy(() => import("./pages/Settings"));
const Notifications = lazy(() => import("./pages/Notifications"));

const NAV = [
  { to: "/", label: "Today", icon: Sun, end: true },
  { to: "/tasks", label: "Tasks", icon: CheckSquare },
  { to: "/focus", label: "Focus", icon: Timer },
  { to: "/calendar", label: "Calendar", icon: CalendarDays },
  { to: "/groups", label: "Groups", icon: Users },
  { to: "/inbox", label: "Inbox", icon: Inbox },
  { to: "/settings", label: "Settings", icon: SettingsIcon },
];

const TITLES: Record<string, string> = {
  "/": "Today",
  "/tasks": "Tasks",
  "/focus": "Focus",
  "/calendar": "Calendar",
  "/groups": "Groups",
  "/inbox": "Inbox",
  "/settings": "Settings",
  "/notifications": "Notifications",
};

function useDocumentTitle() {
  const { pathname } = useLocation();
  const first = useRef(true);
  useEffect(() => {
    const base = "/" + (pathname.split("/")[1] ?? "");
    document.title = `${TITLES[base] ?? "Tendly"} · Tendly`;
    // After in-app navigation, move focus to the main region for screen readers.
    // Not on first load, so the skip link stays the first Tab stop.
    if (first.current) {
      first.current = false;
      return;
    }
    document.getElementById("main")?.focus({ preventScroll: true });
  }, [pathname]);
}

function Shell() {
  useDocumentTitle();
  const me = useMe();
  const { data: count } = useQuery({ queryKey: keys.inboxCount, queryFn: api.inboxCount, refetchInterval: 120_000 });
  const { data: notes = [] } = useQuery({ queryKey: keys.notifications, queryFn: api.notifications, refetchInterval: 60_000 });
  const unread = notes.filter((n) => !n.readAt).length;
  return (
    <div className="shell">
      <a href="#main" className="skip-link">
        Skip to content
      </a>
      <aside className="sidebar" aria-label="Main">
        <NavLink to="/" className="brand" aria-label="Tendly home">
          <Mascot size={36} animated={false} />
          Tendly
        </NavLink>
        <nav className="nav" aria-label="Sections">
          {NAV.map(({ to, label, icon: Icon, end }) => (
            <NavLink key={to} to={to} end={end}>
              <Icon size={20} aria-hidden />
              {label}
              {to === "/inbox" && count && count.pending > 0 ? (
                <span className="count">
                  {count.pending}
                  <span className="visually-hidden"> suggestions to review</span>
                </span>
              ) : null}
            </NavLink>
          ))}
          <NavLink to="/notifications">
            <Bell size={20} aria-hidden />
            Notifications
            {unread > 0 && (
              <span className="count">
                {unread}
                <span className="visually-hidden"> unread</span>
              </span>
            )}
          </NavLink>
        </nav>
        <div className="sidebar-foot">
          {me && <span>Hi, {me.displayName}</span>}
        </div>
      </aside>
      <header className="topbar">
        <NavLink to="/" className="brand" aria-label="Tendly home">
          <Mascot size={30} animated={false} />
          Tendly
        </NavLink>
        <div className="topbar-actions">
          <NavLink to="/inbox" className="btn btn-ghost btn-icon" aria-label={`Inbox${count?.pending ? `, ${count.pending} to review` : ""}`}>
            <Inbox size={20} aria-hidden />
          </NavLink>
          <NavLink to="/notifications" className="btn btn-ghost btn-icon" aria-label={`Notifications${unread ? `, ${unread} unread` : ""}`}>
            <Bell size={20} aria-hidden />
          </NavLink>
          <NavLink to="/settings" className="btn btn-ghost btn-icon" aria-label="Settings">
            <SettingsIcon size={20} aria-hidden />
          </NavLink>
        </div>
      </header>
      <main id="main" className="main" tabIndex={-1}>
        <Suspense fallback={<Loading />}>
          <Routes>
            <Route path="/" element={<Today />} />
            <Route path="/tasks" element={<Tasks />} />
            <Route path="/focus" element={<Focus />} />
            <Route path="/calendar" element={<Calendar />} />
            <Route path="/groups" element={<Groups />} />
            <Route path="/groups/:id" element={<GroupDetail />} />
            <Route path="/inbox" element={<InboxPage />} />
            <Route path="/settings" element={<Settings />} />
            <Route path="/notifications" element={<Notifications />} />
            <Route path="*" element={<p>That page doesn't exist.</p>} />
          </Routes>
        </Suspense>
      </main>
      <nav className="tabbar" aria-label="Sections">
        {NAV.slice(0, 5).map(({ to, label, icon: Icon, end }) => (
          <NavLink key={to} to={to} end={end}>
            <Icon size={22} aria-hidden />
            {label}
          </NavLink>
        ))}
      </nav>
      <TimerWatcher />
      <AlarmWatcher />
    </div>
  );
}

export function App() {
  const session = useSession();
  const actorId = useActorId();
  const members = useMembers();
  if (session.isLoading) return <Loading label="Starting Tendly…" />;
  if (session.error)
    return (
      <main className="main">
        <ErrorState message="Tendly's server isn't responding. Is it running? (tendly serve)" onRetry={() => session.refetch()} />
      </main>
    );
  if (session.data?.deviceAuthRequired && !session.data.devicePaired) return <PairDevice />;
  if (members.isLoading) return <Loading />;
  const known = members.data?.some((m) => m.id === actorId);
  if (!actorId || !known) return <Onboarding />;
  return <Shell />;
}
