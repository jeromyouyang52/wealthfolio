import { invoke, logger } from "#platform";
import type { HolderProfile, RoomReport } from "../types/contribution-room";

/**
 * Computes each holder's TFSA, RRSP and FHSA room. Like the tax report, this
 * may ask the Bank of Canada for rates of foreign-currency deposits; a failed
 * request comes back inside the report.
 */
export const getContributionRoom = async (): Promise<RoomReport> => {
  try {
    return await invoke<RoomReport>("get_contribution_room");
  } catch (e) {
    logger.error("Error computing the contribution room.");
    throw e;
  }
};

/** Saves the figures a holder entered. An empty profile clears them. */
export const saveHolderProfile = async (
  holder: string,
  profile: HolderProfile,
): Promise<HolderProfile> => {
  try {
    return await invoke<HolderProfile>("save_holder_profile", { holder, profile });
  } catch (e) {
    logger.error("Error saving the contribution room figures.");
    throw e;
  }
};
