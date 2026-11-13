import { invoke, logger } from "#platform";
import type { TaxReport } from "../types/tax-report";

/**
 * Computes the Canadian tax report. This may ask the Bank of Canada for
 * exchange rates not stored yet; a failed request comes back inside the
 * report (`rateFailures`), not as an error.
 */
export const getTaxReport = async (): Promise<TaxReport> => {
  try {
    return await invoke<TaxReport>("get_tax_report");
  } catch (e) {
    logger.error("Error computing the tax report.");
    throw e;
  }
};
