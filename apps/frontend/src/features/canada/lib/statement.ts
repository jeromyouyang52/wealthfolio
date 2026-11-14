// The annual capital gains statement: one holder's dispositions and deemed
// gains of one year, laid out as Schedule 3 asks for them. Everything here is
// read from the tax report; nothing is computed again.

import type { ExportCell } from "@/pages/allocation-targets/components/allocation-worksheet-export";
import type { HolderReport, TaxIssue, TaxReport, TaxYearSummary } from "../types/tax-report";

export interface StatementLine {
  key: string;
  date: string;
  securityId: string;
  /** A return of capital beyond the cost, which has no sale. */
  deemed: boolean;
  units: number | null;
  proceedsCad: number | null;
  acbCad: number | null;
  /** Before the superficial loss rule. */
  gainBeforeRuleCad: number;
  deniedCad: number;
  deniedCarriedCad: number;
  deniedLostCad: number;
  /** What goes on the return. */
  reportableCad: number;
  provisional: boolean;
  needsReview: boolean;
}

/** Years with something to report for any holder, newest first. */
export function statementYears(report: TaxReport): number[] {
  const years = new Set<number>();
  for (const holder of report.holders) for (const y of holder.years) years.add(y.year);
  return [...years].sort((a, b) => b - a);
}

export function statementLines(holder: HolderReport, year: number): StatementLine[] {
  const inYear = (date: string) => Number(date.slice(0, 4)) === year;
  const lines: StatementLine[] = holder.positions.flatMap((position) => [
    ...position.dispositions
      .filter((d) => inYear(d.date))
      .map((d, index) => ({
        key: `${position.securityId}-d-${index}`,
        date: d.date,
        securityId: position.securityId,
        deemed: false,
        units: d.units,
        proceedsCad: d.netProceedsCad,
        acbCad: d.acbRelievedCad,
        gainBeforeRuleCad: d.gainCad,
        deniedCad: d.deniedLossCad,
        deniedCarriedCad: d.deniedAddedToAcbCad,
        deniedLostCad: d.deniedLostCad,
        reportableCad: d.reportableGainCad,
        provisional: d.provisional,
        needsReview: d.needsReview,
      })),
    ...position.deemedGains
      .filter((g) => inYear(g.date))
      .map((g, index) => ({
        key: `${position.securityId}-g-${index}`,
        date: g.date,
        securityId: position.securityId,
        deemed: true,
        units: null,
        proceedsCad: null,
        acbCad: null,
        gainBeforeRuleCad: g.gainCad,
        deniedCad: 0,
        deniedCarriedCad: 0,
        deniedLostCad: 0,
        reportableCad: g.gainCad,
        provisional: false,
        needsReview: false,
      })),
  ]);
  // ISO days sort as text; oldest first, as on a schedule.
  return lines.sort((a, b) => a.date.localeCompare(b.date) || a.key.localeCompare(b.key));
}

export function yearSummary(holder: HolderReport, year: number): TaxYearSummary | null {
  return holder.years.find((summary) => summary.year === year) ?? null;
}

/** Issues that can change this holder's figures: theirs, and those of no holder. */
export function holderIssues(report: TaxReport, holder: string): TaxIssue[] {
  return report.issues.filter((issue) => issue.holder === null || issue.holder === holder);
}

/** Every text the CSV carries, already translated by the caller. */
export interface StatementCsvLabels {
  title: string;
  holder: string;
  exportedOn: string;
  currency: string;
  date: string;
  security: string;
  units: string;
  proceeds: string;
  acb: string;
  gainBeforeRule: string;
  denied: string;
  reportable: string;
  note: string;
  deemed: string;
  gains: string;
  losses: string;
  deniedTotal: string;
  net: string;
  taxable: string;
  leftOut: string;
  disclaimer: string;
  /** The note for a line: denied amounts, provisional, review. */
  lineNote: (line: StatementLine) => string;
  /** One left-out security, in words. */
  issue: (issue: TaxIssue) => string;
  securityName: (securityId: string) => string;
}

const money = (value: number) => ({ decimal: value, digits: 2 });

export function statementCsvRows(
  lines: StatementLine[],
  summary: TaxYearSummary | null,
  issues: TaxIssue[],
  context: { holderName: string; exportedOn: string },
  labels: StatementCsvLabels,
): ExportCell[][] {
  const rows: ExportCell[][] = [
    [labels.title],
    [labels.holder, context.holderName],
    [labels.exportedOn, context.exportedOn],
    [labels.currency, "CAD"],
    [],
    [
      labels.date,
      labels.security,
      labels.units,
      labels.proceeds,
      labels.acb,
      labels.gainBeforeRule,
      labels.denied,
      labels.reportable,
      labels.note,
    ],
  ];
  for (const line of lines) {
    rows.push([
      line.date,
      labels.securityName(line.securityId),
      line.units ?? "",
      line.proceedsCad === null ? "" : money(line.proceedsCad),
      line.acbCad === null ? "" : money(line.acbCad),
      money(line.gainBeforeRuleCad),
      money(line.deniedCad),
      money(line.reportableCad),
      line.deemed ? labels.deemed : labels.lineNote(line),
    ]);
  }
  if (summary) {
    rows.push(
      [],
      [labels.gains, money(summary.gainsCad)],
      [labels.losses, money(summary.lossesCad)],
      [labels.deniedTotal, money(summary.deniedLossesCad)],
      [labels.net, money(summary.netCad)],
      [labels.taxable, money(summary.taxableCad)],
    );
  }
  if (issues.length > 0) {
    rows.push([], [labels.leftOut]);
    for (const issue of issues) {
      rows.push([labels.securityName(issue.securityId), labels.issue(issue)]);
    }
  }
  rows.push([], [labels.disclaimer]);
  return rows;
}

export function statementFileName(year: number, holder: string): string {
  const safeHolder = holder.replace(/[^\p{L}\p{N}_-]+/gu, "-");
  return `loontax-capital-gains-${year}-${safeHolder}.csv`;
}
