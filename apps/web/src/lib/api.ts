import type {
  AcceptSuggestionInput,
  Activity,
  AdminSettings,
  AdminSettingsInput,
  AiSettings,
  AiSettingsInput,
  Alarm,
  AlarmInput,
  CalendarEvent,
  CalendarEventInput,
  CalendarEventPatch,
  CalendarSource,
  CalendarSourceInput,
  CalendarSourcePatch,
  CompleteTaskResult,
  Connector,
  ConnectorInput,
  ConnectorPatch,
  ConnectorSummary,
  Countdown,
  CountdownInput,
  CreatedShareLink,
  EventOccurrence,
  Group,
  GroupInput,
  GroupPatch,
  ImportResult,
  IntakeInput,
  IntakeResult,
  Member,
  MemberInput,
  MemberPatch,
  MilestoneInput,
  Notification,
  NudgeInput,
  ProviderInfo,
  RoutineTemplate,
  SessionInfo,
  ShareLink,
  ShareLinkInput,
  Suggestion,
  Task,
  TaskHistory,
  TaskInput,
  TemplatePatch,
  TimerCommand,
  TimerView,
  UsageInput,
  UsageView,
  UseTemplateInput,
} from "@tendly/contracts";
import { send } from "./transport";
import { getActorId } from "./prefs";

export class ApiError extends Error {
  status: number;
  code: string;
  field?: string;
  current?: unknown;
  constructor(status: number, code: string, message: string, field?: string, current?: unknown) {
    super(message);
    this.status = status;
    this.code = code;
    this.field = field;
    this.current = current;
  }
}

let adminToken: string | null = null;
/** Remote-mode admin token. Kept in memory only, never persisted. */
export function setAdminToken(t: string | null) {
  adminToken = t;
}

export async function request<T>(method: string, path: string, body?: unknown, contentType = "application/json"): Promise<T> {
  const headers: Record<string, string> = { accept: "application/json", "x-tendly-csrf": "1" };
  const actor = getActorId();
  if (actor) headers["x-tendly-actor"] = actor;
  if (adminToken && path.startsWith("/api/admin/")) headers["x-tendly-admin-token"] = adminToken;
  let payload: string | undefined;
  if (body !== undefined) {
    headers["content-type"] = contentType;
    payload = typeof body === "string" ? body : JSON.stringify(body);
  }
  let res;
  try {
    res = await send(method, path, headers, payload);
  } catch {
    throw new ApiError(0, "offline", "Tendly's server can't be reached right now. Your changes weren't saved — try again in a moment.");
  }
  let data: unknown = null;
  if (res.body) {
    try {
      data = JSON.parse(res.body);
    } catch {
      data = res.body;
    }
  }
  if (res.status < 200 || res.status >= 300) {
    const e = (data && typeof data === "object" ? data : {}) as { code?: string; message?: string; field?: string; current?: unknown };
    throw new ApiError(res.status, e.code ?? "error", e.message ?? `Request failed (${res.status}).`, e.field, e.current);
  }
  return data as T;
}

const get = <T>(p: string) => request<T>("GET", p);
const post = <T>(p: string, b?: unknown) => request<T>("POST", p, b ?? {});
const patch = <T>(p: string, b: unknown) => request<T>("PATCH", p, b);
const put = <T>(p: string, b: unknown) => request<T>("PUT", p, b);
const del = <T>(p: string) => request<T>("DELETE", p);
const q = (params: Record<string, string | number | boolean | undefined | null>) => {
  const s = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) if (v !== undefined && v !== null && v !== "") s.set(k, String(v));
  const str = s.toString();
  return str ? `?${str}` : "";
};

export type TaskFilters = {
  groupId?: string;
  assigneeId?: string;
  category?: string;
  tag?: string;
  status?: "open" | "done" | "all";
  personal?: boolean;
  dueBefore?: string;
  dueAfter?: string;
  q?: string;
};

export type RangeFilters = {
  from: string;
  to: string;
  groupId?: string;
  memberId?: string;
  category?: string;
  sourceId?: string;
  includeTasks?: boolean;
};

export const api = {
  session: () => get<SessionInfo>("/api/session"),
  pair: (code: string) => post<{ paired: boolean }>("/api/auth/pair", { code }),

  members: () => get<Member[]>("/api/members"),
  createMember: (i: MemberInput) => post<Member>("/api/members", i),
  updateMember: (id: string, p: MemberPatch) => patch<Member>(`/api/members/${id}`, p),

  groups: () => get<Group[]>("/api/groups"),
  group: (id: string) => get<Group>(`/api/groups/${id}`),
  createGroup: (i: GroupInput) => post<Group>("/api/groups", i),
  updateGroup: (id: string, p: GroupPatch) => patch<Group>(`/api/groups/${id}`, p),
  joinGroup: (id: string) => post<Group>(`/api/groups/${id}/join`),
  leaveGroup: (id: string) => post<Group>(`/api/groups/${id}/leave`),
  addMilestone: (id: string, i: MilestoneInput) => post<Group>(`/api/groups/${id}/milestones`, i),
  updateMilestone: (id: string, mid: string, i: MilestoneInput) => patch<Group>(`/api/groups/${id}/milestones/${mid}`, i),
  deleteMilestone: (id: string, mid: string) => del<Group>(`/api/groups/${id}/milestones/${mid}`),

  tasks: (f: TaskFilters = {}) => get<Task[]>(`/api/tasks${q(f)}`),
  task: (id: string) => get<Task>(`/api/tasks/${id}`),
  createTask: (i: TaskInput) => post<Task>("/api/tasks", i),
  updateTask: (id: string, expectedVersion: number, changes: Partial<Task>) => patch<Task>(`/api/tasks/${id}`, { expectedVersion, changes }),
  completeTask: (id: string) => post<CompleteTaskResult>(`/api/tasks/${id}/complete`),
  reopenTask: (id: string) => post<Task>(`/api/tasks/${id}/reopen`),
  moveTask: (id: string, expectedVersion: number, columnKey: string, position?: number) =>
    post<Task>(`/api/tasks/${id}/move`, { expectedVersion, columnKey, position }),
  deleteTask: (id: string) => del<{ deleted: boolean }>(`/api/tasks/${id}`),
  taskHistory: (id: string) => get<TaskHistory>(`/api/tasks/${id}/history`),

  templates: () => get<RoutineTemplate[]>("/api/templates"),
  updateTemplate: (key: string, p: TemplatePatch) => patch<RoutineTemplate>(`/api/templates/${key}`, p),
  useTemplate: (key: string, i: UseTemplateInput) => post<Task>(`/api/templates/${key}/use`, i),

  activity: (f: { groupId?: string; entityType?: string; limit?: number } = {}) => get<Activity[]>(`/api/activity${q(f)}`),
  notifications: () => get<Notification[]>("/api/notifications"),
  markRead: (id: string) => post(`/api/notifications/${id}/read`),
  markAllRead: () => post("/api/notifications/read-all"),
  nudge: (i: NudgeInput) => post<{ deliverAfter: string; deferred: boolean }>("/api/nudges", i),

  timer: () => get<TimerView>("/api/timer"),
  timerCommand: (command: TimerCommand, expectedVersion?: number) => post<TimerView>("/api/timer", { command, expectedVersion }),
  countdowns: () => get<Countdown[]>("/api/countdowns"),
  createCountdown: (i: CountdownInput) => post<{ id: string }>("/api/countdowns", i),
  deleteCountdown: (id: string) => del(`/api/countdowns/${id}`),
  alarms: () => get<Alarm[]>("/api/alarms"),
  createAlarm: (i: AlarmInput) => post<Alarm>("/api/alarms", i),
  updateAlarm: (id: string, i: AlarmInput) => patch<Alarm>(`/api/alarms/${id}`, i),
  deleteAlarm: (id: string) => del(`/api/alarms/${id}`),
  alarmFired: (id: string) => post<Alarm>(`/api/alarms/${id}/fired`),
  snoozeAlarm: (id: string, minutes: number) => post<Alarm>(`/api/alarms/${id}/snooze`, { minutes }),
  usage: () => get<UsageView>("/api/usage"),
  putUsage: (i: UsageInput) => put<UsageView>("/api/usage", i),

  sources: () => get<CalendarSource[]>("/api/calendar/sources"),
  createSource: (i: CalendarSourceInput) => post<CalendarSource>("/api/calendar/sources", i),
  updateSource: (id: string, p: CalendarSourcePatch) => patch<CalendarSource>(`/api/calendar/sources/${id}`, p),
  deleteSource: (id: string) => del(`/api/calendar/sources/${id}`),
  refreshSource: (id: string) => post<ImportResult>(`/api/calendar/sources/${id}/refresh`),
  importIcs: (text: string, params: { sourceId?: string; name?: string; groupId?: string; isPrivate?: boolean }) =>
    request<ImportResult>("POST", `/api/calendar/import${q(params)}`, text, "text/calendar"),
  occurrences: (f: RangeFilters) => get<EventOccurrence[]>(`/api/calendar/occurrences${q(f)}`),
  exportUrl: (kind: "ics" | "json", f: RangeFilters) => `/api/calendar/export.${kind}${q(f)}`,
  exportText: (kind: "ics" | "json", f: RangeFilters) => request<unknown>("GET", `/api/calendar/export.${kind}${q(f)}`),
  event: (id: string) => get<CalendarEvent>(`/api/calendar/events/${id}`),
  createEvent: (i: CalendarEventInput) => post<CalendarEvent>("/api/calendar/events", i),
  updateEvent: (id: string, p: CalendarEventPatch) => patch<CalendarEvent>(`/api/calendar/events/${id}`, p),
  calendarChanges: (sourceId?: string) => get<Activity[]>(`/api/calendar/changes${q({ sourceId })}`),
  shares: () => get<ShareLink[]>("/api/shares"),
  createShare: (i: ShareLinkInput) => post<CreatedShareLink>("/api/shares", i),
  revokeShare: (id: string) => post<ShareLink>(`/api/shares/${id}/revoke`),

  suggestions: (status = "pending") => get<Suggestion[]>(`/api/inbox/suggestions${q({ status })}`),
  intake: (i: IntakeInput) => post<IntakeResult>("/api/inbox/intake", i),
  acceptSuggestion: (id: string, i: AcceptSuggestionInput) => post<Suggestion>(`/api/inbox/suggestions/${id}/accept`, i),
  dismissSuggestion: (id: string) => post<Suggestion>(`/api/inbox/suggestions/${id}/dismiss`),
  myConnectors: () => get<ConnectorSummary[]>("/api/inbox/connectors"),
  inboxCount: () => get<{ pending: number }>("/api/inbox/count"),

  admin: {
    settings: () => get<AdminSettings>("/api/admin/settings"),
    patchSettings: (i: AdminSettingsInput) => patch<AdminSettings>("/api/admin/settings", i),
    ai: () => get<AiSettings>("/api/admin/ai"),
    putAi: (i: AiSettingsInput) => put<AiSettings>("/api/admin/ai", i),
    testAi: () => post<{ ok: boolean; error?: string; suggestions?: number }>("/api/admin/ai/test"),
    providers: () => get<ProviderInfo[]>("/api/admin/connectors/providers"),
    connectors: () => get<Connector[]>("/api/admin/connectors"),
    createConnector: (i: ConnectorInput) => post<Connector>("/api/admin/connectors", i),
    patchConnector: (id: string, p: ConnectorPatch) => patch<Connector>(`/api/admin/connectors/${id}`, p),
    deleteConnector: (id: string) => del(`/api/admin/connectors/${id}`),
    runConnector: (id: string) => post<Connector>(`/api/admin/connectors/${id}/run`),
    disconnect: (id: string) => post<Connector>(`/api/admin/connectors/${id}/disconnect`),
    setToken: (id: string, token: string) => post<Connector>(`/api/admin/connectors/${id}/credentials`, { token }),
    oauthStart: (provider: string, connectorId: string) => get<{ url: string }>(`/api/admin/oauth/${provider}/start${q({ connectorId })}`),
    devices: () => get<{ id: string; name: string; createdAt: string; lastSeenAt: string | null; revokedAt: string | null }[]>("/api/admin/devices"),
    createDevice: (name: string) => post<{ id: string; name: string; code: string }>("/api/admin/devices", { name }),
    revokeDevice: (id: string) => post(`/api/admin/devices/${id}/revoke`),
    jobs: () => get<{ id: string; kind: string; status: string; attempts: number; runAfter: string; lastError: string | null; updatedAt: string }[]>("/api/admin/jobs"),
    exportAll: () => get<unknown>("/api/admin/export"),
  },
};

export function errorMessage(e: unknown): string {
  if (e instanceof ApiError) return e.message;
  if (e instanceof Error) return e.message;
  return "Something went wrong.";
}
