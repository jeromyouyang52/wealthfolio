import { describe, expect, it } from "vitest";

import { toCsv } from "@/pages/allocation-targets/components/allocation-worksheet-export";
import type { HolderReport, TaxDisposition, TaxReport } from "../types/tax-report";
import {
  duplicateSecurityCodes,
  holderIssues,
  statementCsvRows,
  statementFileName,
  statementLines,
  statementYears,
  yearSummary,
  type StatementCsvLabels,
} from "./statement";

const sale = (date: string, gain: number, denied = 0): TaxDisposition => ({
  date,
  units: 10,
  netProceedsCad: 1000 + gain,
  acbRelievedCad: 1000,
  gainCad: gain,
  deniedLossCad: denied,
  deniedAddedToAcbCad: denied,
  deniedLostCad: 0,
  reportableGainCad: gain + denied,
  provisional: false,
  needsReview: false,
});

const holder: HolderReport = {
  holder: "self",
  years: [
    {
      year: 2026,
      gainsCad: 350,
      lossesCad: 0,
      deniedLossesCad: 100,
      netCad: 350,
      taxableCad: 175,
      provisional: false,
      needsReview: false,
    },
    {
      year: 2025,
      gainsCad: 50,
      lossesCad: 0,
      deniedLossesCad: 0,
      netCad: 50,
      taxableCad: 25,
      provisional: false,
      needsReview: false,
    },
  ],
  yearsWithoutRate: [],
  positions: [
    {
      securityId: "XEQT",
      units: 0,
      acbTotalCad: 0,
      acbPerUnitCad: null,
      dispositions: [sale("2026-06-02", 300), sale("2025-03-01", 50)],
      deemedGains: [{ date: "2026-12-15", gainCad: 50 }],
    },
    {
      securityId: "VCN",
      units: 0,
      acbTotalCad: 0,
      acbPerUnitCad: null,
      dispositions: [sale("2026-02-10", -100, 100)],
      deemedGains: [],
    },
  ],
};

const report: TaxReport = {
  asOf: "2026-11-13",
  holders: [holder, { ...holder, holder: "spouse", years: [], positions: [] }],
  issues: [
    {
      securityId: "ZAG",
      holder: "self",
      code: "UNCLASSIFIED_ACCOUNT",
      accountId: "a",
      activityId: null,
      currency: null,
      date: null,
      detail: null,
    },
    {
      securityId: "VTI",
      holder: "spouse",
      code: "UNREADABLE_ACTIVITY",
      accountId: null,
      activityId: "x",
      currency: null,
      date: null,
      detail: null,
    },
  ],
  fxFallbacks: [],
  rateFailures: [],
  unreadableAccountIds: [],
};

const labels: StatementCsvLabels = {
  title: "Statement 2026",
  holder: "Holder",
  exportedOn: "Prepared on",
  currency: "Currency",
  date: "Date",
  security: "Security",
  units: "Units",
  proceeds: "Proceeds",
  acb: "ACB",
  gainBeforeRule: "Before rule",
  denied: "Denied",
  reportable: "To report",
  note: "Note",
  deemed: "Return of capital",
  gains: "Gains",
  losses: "Losses",
  deniedTotal: "Denied losses",
  net: "Net",
  taxable: "Taxable",
  leftOut: "Left out",
  disclaimer: "Not advice",
  lineNote: (line) => (line.deniedCad > 0 ? "carried" : ""),
  issue: (issue) => issue.code,
  securityName: (id) => id,
};

describe("the annual statement", () => {
  it("lists one holder's year, oldest first, with deemed gains", () => {
    const lines = statementLines(holder, 2026);
    expect(lines.map((l) => [l.date, l.securityId, l.deemed, l.reportableCad])).toEqual([
      ["2026-02-10", "VCN", false, 0],
      ["2026-06-02", "XEQT", false, 300],
      ["2026-12-15", "XEQT", true, 50],
    ]);
    expect(statementYears(report)).toEqual([2026, 2025]);
    expect(yearSummary(holder, 2025)?.taxableCad).toBe(25);
    expect(holderIssues(report, "self").map((i) => i.securityId)).toEqual(["ZAG"]);
  });

  it("writes a CSV a spreadsheet reads, with the year's totals", () => {
    const rows = statementCsvRows(
      statementLines(holder, 2026),
      yearSummary(holder, 2026),
      holderIssues(report, "self"),
      { holderName: "Me", exportedOn: "2026-11-13" },
      labels,
    );
    const csv = toCsv(rows).split("\n");
    expect(csv[5]).toBe(
      '"Date","Security","Units","Proceeds","ACB","Before rule","Denied","To report","Note"',
    );
    expect(csv[6]).toBe(
      '"2026-02-10","VCN","10","900.00","1000.00","-100.00","100.00","0.00","carried"',
    );
    expect(csv[8]).toBe('"2026-12-15","XEQT","","","","50.00","0.00","50.00","Return of capital"');
    expect(csv).toContain('"Taxable","175.00"');
    expect(csv).toContain('"ZAG","UNCLASSIFIED_ACCOUNT"');
    expect(csv.at(-1)).toBe('"Not advice"');

    // A comma decimal mark switches the separator, as the other exports do.
    expect(toCsv(rows, ",").split("\n")[6]).toContain('"900,00";"1000,00"');
  });

  it("names the file after the year and holder", () => {
    expect(statementFileName(2026, "self")).toBe("loontax-capital-gains-2026-self.csv");
    expect(statementFileName(2026, "Jean / Marie")).toBe(
      "loontax-capital-gains-2026-Jean-Marie.csv",
    );
  });

  it("finds one code standing for several securities", () => {
    const twice: HolderReport = {
      ...holder,
      positions: [...holder.positions, { ...holder.positions[0], securityId: "uuid-of-xeqt" }],
    };
    const code = (id: string) => (id === "uuid-of-xeqt" ? "xeqt" : id);
    expect(duplicateSecurityCodes(twice, code)).toEqual(["XEQT"]);
    expect(duplicateSecurityCodes(holder, code)).toEqual([]);
  });
});
