import { useQuery } from "@tanstack/react-query";

import { getTaxReport } from "../adapters/tax-report";
import type { TaxReport } from "../types/tax-report";

export const TAX_REPORT_QUERY_KEY = "loontaxTaxReport";

/**
 * The Canadian tax report. It is computed from every account and activity,
 * and may fetch exchange rates, so it is never served from a stale cache:
 * each visit to the page computes it again.
 */
export function useTaxReport() {
  return useQuery<TaxReport, Error>({
    queryKey: [TAX_REPORT_QUERY_KEY],
    queryFn: getTaxReport,
    staleTime: 0,
    gcTime: 0,
    refetchOnWindowFocus: false,
  });
}
