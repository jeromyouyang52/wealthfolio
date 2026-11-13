import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { getContributionRoom, saveHolderProfile } from "../adapters/contribution-room";
import type { HolderProfile, RoomReport } from "../types/contribution-room";

export const CONTRIBUTION_ROOM_QUERY_KEY = "loontaxContributionRoom";

/** Each holder's room, computed again on every visit, as the tax report is. */
export function useContributionRoom() {
  return useQuery<RoomReport, Error>({
    queryKey: [CONTRIBUTION_ROOM_QUERY_KEY],
    queryFn: getContributionRoom,
    staleTime: 0,
    gcTime: 0,
    refetchOnWindowFocus: false,
  });
}

export function useSaveHolderProfile() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ holder, profile }: { holder: string; profile: HolderProfile }) =>
      saveHolderProfile(holder, profile),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: [CONTRIBUTION_ROOM_QUERY_KEY] }),
  });
}
