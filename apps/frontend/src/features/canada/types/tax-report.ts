// The Canadian tax report, as `loontax-canada-tax-service` returns it.
// Amounts are Canadian dollars; days are ISO dates (YYYY-MM-DD).

export interface TaxReport {
  asOf: string;
  holders: HolderReport[];
  /** Securities left out, and why. Their figures are in no total. */
  issues: TaxIssue[];
  /** Conversions made with an earlier day's Bank of Canada rate. */
  fxFallbacks: FxFallback[];
  /** Rate requests to the Bank of Canada that failed this time. */
  rateFailures: RateFailure[];
  /** Accounts whose stored registration could not be read. */
  unreadableAccountIds: string[];
}

export interface HolderReport {
  holder: string;
  years: TaxYearSummary[];
  /** Years with gains or losses but no inclusion rate in the rule table. */
  yearsWithoutRate: number[];
  positions: TaxPosition[];
}

export interface TaxYearSummary {
  year: number;
  gainsCad: number;
  /** Losses that count, after the superficial loss rule. */
  lossesCad: number;
  /** Losses the rule denied; not part of the net. */
  deniedLossesCad: number;
  netCad: number;
  /** Taxable capital gain when positive, allowable capital loss when negative. */
  taxableCad: number;
  /** A loss in it was sold less than 30 days before `asOf`. */
  provisional: boolean;
  needsReview: boolean;
}

export interface TaxPosition {
  securityId: string;
  units: number;
  acbTotalCad: number;
  acbPerUnitCad: number | null;
  dispositions: TaxDisposition[];
  deemedGains: DeemedGain[];
}

export interface TaxDisposition {
  date: string;
  units: number;
  netProceedsCad: number;
  acbRelievedCad: number;
  /** Before the superficial loss rule; negative for a loss. */
  gainCad: number;
  deniedLossCad: number;
  deniedAddedToAcbCad: number;
  /** Denied and gone for good: the shares were kept in a registered plan. */
  deniedLostCad: number;
  /** The loss less its denied part, or the gain: what Schedule 3 shows. */
  reportableGainCad: number;
  provisional: boolean;
  needsReview: boolean;
}

export interface DeemedGain {
  date: string;
  gainCad: number;
}

export type TaxIssueCode =
  | "UNCLASSIFIED_ACCOUNT"
  | "MISSING_FX_RATE"
  | "UNSUPPORTED_TRANSFER"
  | "UNREADABLE_ACTIVITY"
  | "CONFLICTING_SPLITS"
  | "INVALID_HISTORY";

export interface TaxIssue {
  securityId: string;
  holder: string | null;
  code: TaxIssueCode;
  accountId: string | null;
  activityId: string | null;
  currency: string | null;
  date: string | null;
  detail: string | null;
}

export interface FxFallback {
  activityId: string;
  currency: string;
  date: string;
  rateDate: string;
}

export interface RateFailure {
  currency: string;
  from: string;
  to: string;
  detail: string;
}
