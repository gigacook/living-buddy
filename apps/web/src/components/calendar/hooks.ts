import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { keys } from "../../lib/queries";

export function useCalendarInvalidate() {
  const qc = useQueryClient();
  return () => {
    qc.invalidateQueries({ queryKey: ["occurrences"] });
    qc.invalidateQueries({ queryKey: keys.sources });
    qc.invalidateQueries({ queryKey: ["calendarChanges"] });
  };
}

export function useSourcesQuery(enabled = true) {
  return useQuery({ queryKey: keys.sources, queryFn: api.sources, enabled });
}
