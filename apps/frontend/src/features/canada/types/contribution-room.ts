/**
 * The contribution room the backend computes (`get_contribution_room`).
 * Amounts are Canadian dollars; dates are calendar days ("YYYY-MM-DD").
 */

export type RoomPlan = "TFSA" | "RRSP" | "FHSA";

export type TfsaStart =
  | { basis: "craRoom"; year: number; room: number | string }
  | { basis: "eligibleSince"; year: number };

export interface RrspStart {
  year: number;
  availableRoom: number | string;
}

export interface FhsaStart {
  openedYear: number;
}

/** What a holder entered from the CRA. Each plan is optional. */
export interface HolderProfile {
  tfsa?: TfsaStart;
  rrsp?: RrspStart;
  fhsa?: FhsaStart;
}

export interface ExcessMonth {
  month: string;
  highestExcessCad: number;
  taxCad: number;
}

export interface TfsaRoom {
  basis: "craRoom" | "eligibleSince";
  year: number;
  roomJan1Cad: number;
  contributionsCad: number;
  withdrawalsCad: number;
  availableCad: number;
  excessCad: number;
  nextYearRoomCad: number | null;
  estimatedTaxCad: number;
  flowsBeforeStart: number;
  years: {
    year: number;
    roomJan1Cad: number;
    contributionsCad: number;
    withdrawalsCad: number;
    excessMonths: ExcessMonth[];
  }[];
}

export interface RrspRoom {
  year: number;
  availableRoomCad: number;
  windowStart: string;
  windowEnd: string;
  contributionsCad: number;
  remainingCad: number;
  cushionCad: number;
  excessMonths: ExcessMonth[];
  estimatedTaxCad: number;
  beforeWindowCad: number;
  afterWindowCad: number;
  outdated: boolean;
  nextYearDollarLimitCad: number | null;
}

export interface FhsaRoom {
  openedYear: number;
  year: number;
  roomCad: number;
  carryforwardCad: number;
  contributionsCad: number;
  rrspTransfersCad: number;
  remainingCad: number;
  excessCad: number;
  lifetimeUsedCad: number;
  lifetimeRemainingCad: number;
  nextYearRoomCad: number;
  estimatedTaxCad: number;
  participationPeriodMayHaveEnded: boolean;
  withdrawalsCad: number;
  years: {
    year: number;
    roomCad: number;
    carryforwardCad: number;
    contributionsCad: number;
    rrspTransfersCad: number;
    excessMonths: ExcessMonth[];
  }[];
}

export interface PlanResult<T> {
  status: "ok" | "needsStart" | "error";
  room: T | null;
  detail: string | null;
  hasAccount: boolean;
  incomplete: boolean;
}

export interface HolderRoom {
  holder: string;
  profile: HolderProfile;
  tfsa: PlanResult<TfsaRoom> | null;
  rrsp: PlanResult<RrspRoom> | null;
  fhsa: PlanResult<FhsaRoom> | null;
}

export type RoomIssueCode =
  | "ASSUMED_DIRECT_TRANSFER"
  | "TRANSFER_UNCLASSIFIED_ACCOUNT"
  | "TRANSFER_OTHER_HOLDER"
  | "UNCONFIRMED_ACTIVITY"
  | "MISSING_AMOUNT"
  | "SPOUSAL_RRSP_CONTRIBUTION"
  | "FHSA_WITHDRAWAL"
  | "MISSING_FX_RATE";

export interface RoomIssue {
  holder: string;
  plan: RoomPlan;
  code: RoomIssueCode;
  activityId: string | null;
  accountId: string | null;
  date: string | null;
  currency: string | null;
  amount: number | null;
}

export interface RoomReport {
  asOf: string;
  holders: HolderRoom[];
  issues: RoomIssue[];
  fxFallbacks: { activityId: string; currency: string; date: string; rateDate: string }[];
  rateFailures: { currency: string; from: string; to: string; detail: string }[];
  unreadableAccountIds: string[];
  unreadableProfiles: boolean;
}
