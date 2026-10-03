import { useQuery, useQueryClient } from "@tanstack/react-query";
import type { Member } from "@tendly/contracts";
import { api, type RangeFilters, type TaskFilters } from "./api";
import { useActorId } from "./prefs";

export const keys = {
  session: ["session"] as const,
  members: ["members"] as const,
  groups: ["groups"] as const,
  group: (id: string) => ["groups", id] as const,
  tasks: (f: TaskFilters) => ["tasks", f] as const,
  templates: ["templates"] as const,
  timer: ["timer"] as const,
  alarms: ["alarms"] as const,
  countdowns: ["countdowns"] as const,
  usage: ["usage"] as const,
  sources: ["sources"] as const,
  occurrences: (f: RangeFilters) => ["occurrences", f] as const,
  shares: ["shares"] as const,
  suggestions: (s: string) => ["suggestions", s] as const,
  inboxCount: ["inboxCount"] as const,
  notifications: ["notifications"] as const,
  activity: (f: object) => ["activity", f] as const,
};

export function useSession() {
  return useQuery({ queryKey: keys.session, queryFn: api.session, staleTime: 60_000 });
}

export function useMembers() {
  return useQuery({ queryKey: keys.members, queryFn: api.members, staleTime: 30_000 });
}

export function useMe(): Member | undefined {
  const id = useActorId();
  const { data } = useMembers();
  return data?.find((m) => m.id === id);
}

export function useMemberMap(): Map<string, Member> {
  const { data } = useMembers();
  return new Map((data ?? []).map((m) => [m.id, m]));
}

export function useGroups() {
  return useQuery({ queryKey: keys.groups, queryFn: api.groups });
}

export function useTasks(f: TaskFilters = {}) {
  return useQuery({ queryKey: keys.tasks(f), queryFn: () => api.tasks(f) });
}

export function useInvalidate() {
  const qc = useQueryClient();
  return (...prefixes: string[]) => Promise.all(prefixes.map((p) => qc.invalidateQueries({ queryKey: [p] })));
}

/** Everything that changes when a task changes. */
export const TASK_RELATED = ["tasks", "occurrences", "groups", "activity", "notifications"];
