import { useTranslation } from "react-i18next";
import { useNavigate, useSearchParams } from "react-router-dom";
import { toast } from "sonner";

import { openFileSaveDialog } from "@/adapters";
import { useBalancePrivacy } from "@/hooks/use-balance-privacy";
import { csvFile, toCsv } from "@/pages/allocation-targets/components/allocation-worksheet-export";
import {
  Button,
  Page,
  PageContent,
  PageHeader,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Skeleton,
  useAmountFormatting,
  useDateFormatting,
  useNumberFormatting,
} from "@wealthfolio/ui";
import { Alert, AlertDescription } from "@wealthfolio/ui/components/ui/alert";
import { Icons } from "@wealthfolio/ui/components/ui/icons";

import { useSecurityNames } from "../hooks/use-security-names";
import { useTaxReport } from "../hooks/use-tax-report";
import {
  holderIssues,
  statementCsvRows,
  statementFileName,
  statementLines,
  statementYears,
  yearSummary,
  type StatementLine,
} from "../lib/statement";
import type { TaxIssue, TaxReport } from "../types/tax-report";

const CAD = "CAD";
const numeric = "px-2 py-1.5 text-end tabular-nums whitespace-nowrap";
const cell = "px-2 py-1.5 align-top";
// Header labels are long; they wrap so the amounts below keep one line.
const numericHead = "px-2 py-1.5 text-end align-bottom font-medium";
const textHead = "px-2 py-1.5 text-start align-bottom font-medium";
// The table is wide: printed pages turn sideways while this page is open.
const PRINT_PAGE = "@page { size: landscape; margin: 12mm; }";

function useHolderName() {
  const { t } = useTranslation();
  return (holder: string) =>
    holder === "self"
      ? t("canada:holder_self")
      : holder === "spouse"
        ? t("canada:holder_spouse")
        : holder;
}

function useIssueText() {
  const { t } = useTranslation();
  const names = useSecurityNames();
  const { formatCalendarDate } = useDateFormatting();
  return (issue: TaxIssue) =>
    t(`canada:issues.${issue.code}`, {
      account: issue.accountId ? names.account(issue.accountId) : "",
      currency: issue.currency ?? "",
      date: issue.date ? formatCalendarDate(issue.date) : "",
      detail: issue.detail ?? "",
    });
}

function useLineNote() {
  const { t } = useTranslation();
  const { formatAmount } = useAmountFormatting();
  return (line: StatementLine, hidden = false) => {
    const amount = (value: number) => (hidden ? "••••" : formatAmount(value, CAD));
    const notes: string[] = [];
    if (line.deniedLostCad > 0) {
      notes.push(t("canada:dispositions.denied_lost", { amount: amount(line.deniedLostCad) }));
    }
    if (line.deniedCarriedCad > 0) {
      notes.push(
        t("canada:dispositions.denied_carried", { amount: amount(line.deniedCarriedCad) }),
      );
    }
    if (line.provisional) notes.push(t("canada:statement.note_provisional"));
    if (line.needsReview) notes.push(t("canada:statement.note_review"));
    return notes.join(" ");
  };
}

function Statement({ report, holder, year }: { report: TaxReport; holder: string; year: number }) {
  const { t } = useTranslation();
  const names = useSecurityNames();
  const holderName = useHolderName();
  const issueText = useIssueText();
  const lineNote = useLineNote();
  const { isBalanceHidden } = useBalancePrivacy();
  const { formatAmount } = useAmountFormatting();
  const { formatQuantity } = useNumberFormatting();
  const { formatCalendarDate } = useDateFormatting();
  const cad = (value: number) => (isBalanceHidden ? "••••" : formatAmount(value, CAD));

  const holderReport = report.holders.find((h) => h.holder === holder);
  if (!holderReport) return null;
  const lines = statementLines(holderReport, year);
  const summary = yearSummary(holderReport, year);
  const issues = holderIssues(report, holder);
  const fallbacks = report.fxFallbacks.filter((f) => Number(f.date.slice(0, 4)) === year);

  return (
    <article className="bg-card text-card-foreground mx-auto grid max-w-5xl gap-6 rounded-xl border p-6 text-sm print:max-w-none print:rounded-none print:border-0 print:bg-transparent print:p-0">
      <style>{PRINT_PAGE}</style>
      {/* Not a <header>: the app's print styles hide those. */}
      <div className="grid gap-1">
        <h1 className="text-xl font-semibold">{t("canada:statement.heading", { year })}</h1>
        <p className="text-muted-foreground">
          {[
            holderName(holder),
            t("canada:statement.prepared_on", { date: formatCalendarDate(report.asOf) }),
            t("canada:statement.in_cad"),
          ].join(" · ")}
        </p>
      </div>

      <p className="text-muted-foreground border-s-2 ps-3 text-xs">{t("canada:disclaimer")}</p>

      {summary && (
        <section className="grid gap-2">
          <h2 className="font-semibold">{t("canada:statement.summary")}</h2>
          <dl className="grid grid-cols-2 gap-x-6 gap-y-1 sm:grid-cols-5 print:grid-cols-5">
            {(
              [
                ["gains", summary.gainsCad],
                ["losses", summary.lossesCad],
                ["denied", summary.deniedLossesCad],
                ["net", summary.netCad],
                ["taxable", summary.taxableCad],
              ] as const
            ).map(([key, value]) => (
              <div key={key} className="grid">
                <dt className="text-muted-foreground text-xs">{t(`canada:years.${key}`)}</dt>
                <dd className="font-medium tabular-nums">{cad(value)}</dd>
              </div>
            ))}
          </dl>
          {summary.provisional && (
            <p className="text-xs">{t("canada:statement.summary_provisional")}</p>
          )}
          {summary.needsReview && <p className="text-xs">{t("canada:statement.summary_review")}</p>}
        </section>
      )}

      <section className="grid gap-2">
        <h2 className="font-semibold">{t("canada:statement.dispositions")}</h2>
        {lines.length === 0 ? (
          <p className="text-muted-foreground">{t("canada:statement.nothing", { year })}</p>
        ) : (
          <div className="overflow-x-auto print:overflow-visible">
            <table className="w-full border-collapse text-xs">
              <thead>
                <tr className="border-b text-start">
                  <th className={textHead}>{t("canada:dispositions.date")}</th>
                  <th className={textHead}>{t("canada:dispositions.security")}</th>
                  <th className={numericHead}>{t("canada:dispositions.units")}</th>
                  <th className={numericHead}>{t("canada:statement.proceeds")}</th>
                  <th className={numericHead}>{t("canada:dispositions.acb")}</th>
                  <th className={numericHead}>{t("canada:statement.gain_before_rule")}</th>
                  <th className={numericHead}>{t("canada:statement.denied")}</th>
                  <th className={numericHead}>{t("canada:statement.reportable")}</th>
                  <th className={textHead}>{t("canada:statement.note")}</th>
                </tr>
              </thead>
              <tbody>
                {lines.map((line) => (
                  <tr key={line.key} className="break-inside-avoid border-b">
                    <td className={`${cell} whitespace-nowrap`}>{formatCalendarDate(line.date)}</td>
                    <td className={`${cell} min-w-40 print:min-w-32`}>
                      <div className="font-medium">{names.security(line.securityId)}</div>
                      {names.description(line.securityId) && (
                        <div className="text-muted-foreground">
                          {names.description(line.securityId)}
                        </div>
                      )}
                    </td>
                    <td className={numeric}>
                      {line.units === null ? "" : formatQuantity(line.units)}
                    </td>
                    <td className={numeric}>
                      {line.proceedsCad === null ? "" : cad(line.proceedsCad)}
                    </td>
                    <td className={numeric}>{line.acbCad === null ? "" : cad(line.acbCad)}</td>
                    <td className={numeric}>{cad(line.gainBeforeRuleCad)}</td>
                    <td className={numeric}>{line.deniedCad > 0 ? cad(line.deniedCad) : ""}</td>
                    <td className={`${numeric} font-medium`}>{cad(line.reportableCad)}</td>
                    <td className={`${cell} text-muted-foreground min-w-48 print:min-w-40`}>
                      {line.deemed
                        ? t("canada:dispositions.deemed")
                        : lineNote(line, isBalanceHidden)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      {issues.length > 0 && (
        <section className="grid gap-2">
          <h2 className="font-semibold">{t("canada:issues.title")}</h2>
          <p className="text-xs">{t("canada:statement.left_out_warning")}</p>
          <ul className="list-disc ps-5 text-xs">
            {issues.map((issue, index) => (
              <li key={`${issue.securityId}-${issue.code}-${index}`}>
                <span className="font-medium">{names.security(issue.securityId)}</span>
                {": "}
                {issueText(issue)}
              </li>
            ))}
          </ul>
        </section>
      )}

      {fallbacks.length > 0 && (
        <section className="grid gap-2">
          <h2 className="font-semibold">{t("canada:fx.title")}</h2>
          <ul className="list-disc ps-5 text-xs">
            {fallbacks.map((fallback) => (
              <li key={fallback.activityId}>
                {t("canada:fx.fallback_row", {
                  currency: fallback.currency,
                  date: formatCalendarDate(fallback.date),
                  rateDate: formatCalendarDate(fallback.rateDate),
                })}
              </li>
            ))}
          </ul>
        </section>
      )}

      <section className="text-muted-foreground grid gap-1 border-t pt-4 text-xs">
        <p>{t("canada:statement.how_to_report")}</p>
        <p>{t("canada:statement.method")}</p>
      </section>
    </article>
  );
}

/** One holder's capital gains for one year, to print, save as PDF or export. */
export default function TaxStatementPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const { data: report, isLoading, isError } = useTaxReport();
  const holderName = useHolderName();
  const issueText = useIssueText();
  const lineNote = useLineNote();
  const names = useSecurityNames();
  const { decimalSeparator } = useNumberFormatting();
  const { formatCalendarDate } = useDateFormatting();

  const years = report ? statementYears(report) : [];
  const requestedYear = Number(params.get("year"));
  const year = years.includes(requestedYear) ? requestedYear : years[0];
  const holders = report?.holders.map((h) => h.holder) ?? [];
  const requestedHolder = params.get("holder");
  const holder =
    requestedHolder && holders.includes(requestedHolder) ? requestedHolder : holders[0];

  const choose = (key: "year" | "holder", value: string) =>
    setParams(
      (next) => {
        next.set(key, value);
        return next;
      },
      { replace: true },
    );

  const exportCsv = async () => {
    if (!report || year === undefined || holder === undefined) return;
    const holderReport = report.holders.find((h) => h.holder === holder);
    if (!holderReport) return;
    try {
      const rows = statementCsvRows(
        statementLines(holderReport, year),
        yearSummary(holderReport, year),
        holderIssues(report, holder),
        { holderName: holderName(holder), exportedOn: report.asOf },
        {
          title: t("canada:statement.heading", { year }),
          holder: t("canada:statement.csv_holder"),
          exportedOn: t("canada:statement.csv_prepared_on"),
          currency: t("canada:statement.csv_currency"),
          date: t("canada:dispositions.date"),
          security: t("canada:dispositions.security"),
          units: t("canada:dispositions.units"),
          proceeds: t("canada:statement.proceeds"),
          acb: t("canada:dispositions.acb"),
          gainBeforeRule: t("canada:statement.gain_before_rule"),
          denied: t("canada:statement.denied"),
          reportable: t("canada:statement.reportable"),
          note: t("canada:statement.note"),
          deemed: t("canada:dispositions.deemed"),
          gains: t("canada:years.gains"),
          losses: t("canada:years.losses"),
          deniedTotal: t("canada:years.denied"),
          net: t("canada:years.net"),
          taxable: t("canada:years.taxable"),
          leftOut: t("canada:issues.title"),
          disclaimer: t("canada:disclaimer"),
          lineNote: (line) => lineNote(line),
          issue: issueText,
          securityName: (id) =>
            [names.security(id), names.description(id)].filter(Boolean).join(" — "),
        },
      );
      // The runtime's own save: a native dialog in the app, a download on the web.
      await openFileSaveDialog(
        csvFile(toCsv(rows, decimalSeparator)),
        statementFileName(year, holder),
      );
    } catch {
      toast.error(t("canada:statement.export_failed"));
    }
  };

  return (
    <Page>
      <PageHeader
        heading={t("canada:statement.title")}
        text={report ? t("canada:as_of", { date: formatCalendarDate(report.asOf) }) : undefined}
        onBack={() => navigate("/tax")}
        className="print:hidden"
        actions={
          report && year !== undefined ? (
            <div className="flex flex-wrap items-center gap-2">
              <Select
                value={String(year)}
                items={years.map((y) => ({ value: String(y), label: String(y) }))}
                onValueChange={(next) => next !== null && choose("year", next)}
              >
                <SelectTrigger aria-label={t("canada:statement.year")} className="w-28">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {years.map((y) => (
                    <SelectItem key={y} value={String(y)}>
                      {y}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              {holders.length > 1 && holder !== undefined && (
                <Select
                  value={holder}
                  items={holders.map((h) => ({ value: h, label: holderName(h) }))}
                  onValueChange={(next) => next !== null && choose("holder", next)}
                >
                  <SelectTrigger aria-label={t("canada:statement.holder")} className="w-40">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {holders.map((h) => (
                      <SelectItem key={h} value={h}>
                        {holderName(h)}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              )}
              <Button variant="outline" onClick={() => void exportCsv()}>
                <Icons.Download className="me-2 h-4 w-4" />
                {t("canada:statement.export_csv")}
              </Button>
              <Button onClick={() => window.print()}>
                <Icons.FileText className="me-2 h-4 w-4" />
                {t("canada:statement.print")}
              </Button>
            </div>
          ) : undefined
        }
      />
      <PageContent>
        {isLoading ? (
          <Skeleton className="mx-auto h-96 w-full max-w-5xl rounded-xl" />
        ) : isError || !report ? (
          <Alert variant="destructive">
            <Icons.AlertTriangle className="h-4 w-4" />
            <AlertDescription>{t("canada:error")}</AlertDescription>
          </Alert>
        ) : year === undefined || holder === undefined ? (
          <p className="text-muted-foreground text-sm">{t("canada:statement.empty")}</p>
        ) : (
          <>
            <p className="text-muted-foreground mx-auto mb-4 max-w-5xl text-xs print:hidden">
              {t("canada:statement.print_hint")}
            </p>
            <Statement report={report} holder={holder} year={year} />
          </>
        )}
      </PageContent>
    </Page>
  );
}
