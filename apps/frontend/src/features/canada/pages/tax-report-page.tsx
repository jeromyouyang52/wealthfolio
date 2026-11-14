import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Link, useSearchParams } from "react-router-dom";

import { useBalancePrivacy } from "@/hooks/use-balance-privacy";
import {
  AnimatedToggleGroup,
  Badge,
  Button,
  Page,
  PageContent,
  PageHeader,
  Skeleton,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
  useAmountFormatting,
  useDateFormatting,
  useNumberFormatting,
} from "@wealthfolio/ui";
import { Alert, AlertDescription } from "@wealthfolio/ui/components/ui/alert";
import { Card, CardContent, CardHeader, CardTitle } from "@wealthfolio/ui/components/ui/card";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@wealthfolio/ui/components/ui/empty";
import { Icons } from "@wealthfolio/ui/components/ui/icons";

import { ContributionRoomView } from "../components/contribution-room-view";
import { DuplicateSecuritiesAlert } from "../components/duplicate-securities-alert";
import { useSecurityNames } from "../hooks/use-security-names";
import { useTaxReport } from "../hooks/use-tax-report";
import type {
  HolderReport,
  TaxDisposition,
  TaxIssue,
  TaxReport,
  TaxYearSummary,
} from "../types/tax-report";

const CAD = "CAD";
const numericCell = "text-end tabular-nums";

function useCad() {
  const amountFormatting = useAmountFormatting();
  const { isBalanceHidden } = useBalancePrivacy();
  return (value: number) => (isBalanceHidden ? "••••" : amountFormatting.formatAmount(value, CAD));
}

function FlagBadge({ label, tip }: { label: string; tip: string }) {
  return (
    <Tooltip>
      <TooltipTrigger render={<Badge variant="warning-subtle" tabIndex={0} />}>
        {label}
      </TooltipTrigger>
      <TooltipContent className="max-w-xs">{tip}</TooltipContent>
    </Tooltip>
  );
}

function Flags({ provisional, needsReview }: { provisional: boolean; needsReview: boolean }) {
  const { t } = useTranslation();
  if (!provisional && !needsReview) return null;
  return (
    <span className="ms-2 inline-flex gap-1 align-middle">
      {provisional && (
        <FlagBadge label={t("canada:years.provisional")} tip={t("canada:years.provisional_tip")} />
      )}
      {needsReview && (
        <FlagBadge
          label={t("canada:years.needs_review")}
          tip={t("canada:years.needs_review_tip")}
        />
      )}
    </span>
  );
}

function YearsTable({ years }: { years: TaxYearSummary[] }) {
  const { t } = useTranslation();
  const cad = useCad();
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>{t("canada:years.year")}</TableHead>
          <TableHead className={numericCell}>{t("canada:years.gains")}</TableHead>
          <TableHead className={numericCell}>{t("canada:years.losses")}</TableHead>
          <TableHead className={numericCell}>{t("canada:years.denied")}</TableHead>
          <TableHead className={numericCell}>{t("canada:years.net")}</TableHead>
          <TableHead className={numericCell}>{t("canada:years.taxable")}</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {[...years].reverse().map((year) => (
          <TableRow key={year.year}>
            <TableCell className="font-medium">
              {year.year}
              <Flags provisional={year.provisional} needsReview={year.needsReview} />
            </TableCell>
            <TableCell className={numericCell}>{cad(year.gainsCad)}</TableCell>
            <TableCell className={numericCell}>{cad(year.lossesCad)}</TableCell>
            <TableCell className={numericCell}>{cad(year.deniedLossesCad)}</TableCell>
            <TableCell className={numericCell}>{cad(year.netCad)}</TableCell>
            <TableCell className={`${numericCell} font-semibold`}>{cad(year.taxableCad)}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}

function PositionsTable({ holder }: { holder: HolderReport }) {
  const { t } = useTranslation();
  const cad = useCad();
  const names = useSecurityNames();
  const { formatQuantity } = useNumberFormatting();
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>{t("canada:positions.security")}</TableHead>
          <TableHead className={numericCell}>{t("canada:positions.units")}</TableHead>
          <TableHead className={numericCell}>{t("canada:positions.acb_total")}</TableHead>
          <TableHead className={numericCell}>{t("canada:positions.acb_per_unit")}</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {holder.positions
          .filter((position) => position.units > 0)
          .map((position) => (
            <TableRow key={position.securityId}>
              <TableCell className="font-medium">{names.security(position.securityId)}</TableCell>
              <TableCell className={numericCell}>{formatQuantity(position.units)}</TableCell>
              <TableCell className={numericCell}>{cad(position.acbTotalCad)}</TableCell>
              <TableCell className={numericCell}>
                {position.acbPerUnitCad === null ? "—" : cad(position.acbPerUnitCad)}
              </TableCell>
            </TableRow>
          ))}
      </TableBody>
    </Table>
  );
}

interface DispositionRow {
  key: string;
  securityId: string;
  date: string;
  disposition?: TaxDisposition;
  deemedGainCad?: number;
}

function DispositionsTable({ holder }: { holder: HolderReport }) {
  const { t } = useTranslation();
  const cad = useCad();
  const names = useSecurityNames();
  const { formatQuantity } = useNumberFormatting();
  const { formatCalendarDate } = useDateFormatting();

  const rows = useMemo(() => {
    const all: DispositionRow[] = holder.positions.flatMap((position) => [
      ...position.dispositions.map((disposition, index) => ({
        key: `${position.securityId}-d-${index}`,
        securityId: position.securityId,
        date: disposition.date,
        disposition,
      })),
      ...position.deemedGains.map((gain, index) => ({
        key: `${position.securityId}-g-${index}`,
        securityId: position.securityId,
        date: gain.date,
        deemedGainCad: gain.gainCad,
      })),
    ]);
    // ISO days sort as text; newest first.
    return all.sort((a, b) => b.date.localeCompare(a.date));
  }, [holder]);

  if (rows.length === 0) return null;
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">{t("canada:dispositions.title")}</CardTitle>
      </CardHeader>
      <CardContent>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>{t("canada:dispositions.date")}</TableHead>
              <TableHead>{t("canada:dispositions.security")}</TableHead>
              <TableHead className={numericCell}>{t("canada:dispositions.units")}</TableHead>
              <TableHead className={numericCell}>{t("canada:dispositions.proceeds")}</TableHead>
              <TableHead className={numericCell}>{t("canada:dispositions.acb")}</TableHead>
              <TableHead className={numericCell}>{t("canada:dispositions.gain")}</TableHead>
              <TableHead>{t("canada:dispositions.denied")}</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {rows.map(({ key, securityId, date, disposition, deemedGainCad }) => (
              <TableRow key={key}>
                <TableCell className="whitespace-nowrap">{formatCalendarDate(date)}</TableCell>
                <TableCell className="font-medium">
                  {names.security(securityId)}
                  {disposition && (
                    <Flags
                      provisional={disposition.provisional}
                      needsReview={disposition.needsReview}
                    />
                  )}
                </TableCell>
                {disposition ? (
                  <>
                    <TableCell className={numericCell}>
                      {formatQuantity(disposition.units)}
                    </TableCell>
                    <TableCell className={numericCell}>{cad(disposition.netProceedsCad)}</TableCell>
                    <TableCell className={numericCell}>{cad(disposition.acbRelievedCad)}</TableCell>
                    <TableCell
                      className={`${numericCell} ${disposition.gainCad < 0 ? "text-destructive" : "text-success"}`}
                    >
                      {cad(disposition.gainCad)}
                    </TableCell>
                    <TableCell className="text-muted-foreground text-xs">
                      {disposition.deniedLostCad > 0 && (
                        <div className="text-destructive">
                          {t("canada:dispositions.denied_lost", {
                            amount: cad(disposition.deniedLostCad),
                          })}
                        </div>
                      )}
                      {disposition.deniedAddedToAcbCad > 0 && (
                        <div>
                          {t("canada:dispositions.denied_carried", {
                            amount: cad(disposition.deniedAddedToAcbCad),
                          })}
                        </div>
                      )}
                    </TableCell>
                  </>
                ) : (
                  <>
                    <TableCell colSpan={3} className="text-muted-foreground text-xs">
                      {t("canada:dispositions.deemed")}
                    </TableCell>
                    <TableCell className={`${numericCell} text-success`}>
                      {cad(deemedGainCad ?? 0)}
                    </TableCell>
                    <TableCell />
                  </>
                )}
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}

function HolderSection({ holder, showName }: { holder: HolderReport; showName: boolean }) {
  const { t } = useTranslation();
  const name =
    holder.holder === "self"
      ? t("canada:holder_self")
      : holder.holder === "spouse"
        ? t("canada:holder_spouse")
        : holder.holder;
  return (
    <section className="grid gap-4">
      {showName && <h2 className="text-lg font-semibold">{name}</h2>}
      <DuplicateSecuritiesAlert holder={holder} />
      {(holder.years.length > 0 || holder.yearsWithoutRate.length > 0) && (
        <Card>
          <CardHeader>
            <CardTitle className="text-base">{t("canada:years.title")}</CardTitle>
          </CardHeader>
          <CardContent className="grid gap-3">
            {holder.years.length > 0 && <YearsTable years={holder.years} />}
            {holder.yearsWithoutRate.length > 0 && (
              <p className="text-muted-foreground text-xs">
                {t("canada:years.without_rate", { years: holder.yearsWithoutRate.join(", ") })}
              </p>
            )}
          </CardContent>
        </Card>
      )}
      {holder.positions.some((position) => position.units > 0) && (
        <Card>
          <CardHeader>
            <CardTitle className="text-base">{t("canada:positions.title")}</CardTitle>
          </CardHeader>
          <CardContent>
            <PositionsTable holder={holder} />
          </CardContent>
        </Card>
      )}
      <DispositionsTable holder={holder} />
    </section>
  );
}

function IssuesCard({ issues }: { issues: TaxIssue[] }) {
  const { t } = useTranslation();
  const names = useSecurityNames();
  const { formatCalendarDate } = useDateFormatting();
  if (issues.length === 0) return null;
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <Icons.AlertTriangle className="text-warning h-4 w-4" />
          {t("canada:issues.title")}
        </CardTitle>
        <p className="text-muted-foreground text-sm">{t("canada:issues.description")}</p>
      </CardHeader>
      <CardContent>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>{t("canada:issues.security")}</TableHead>
              <TableHead>{t("canada:issues.reason")}</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {issues.map((issue, index) => (
              <TableRow key={`${issue.securityId}-${issue.code}-${index}`}>
                <TableCell className="font-medium">{names.security(issue.securityId)}</TableCell>
                <TableCell>
                  {t(`canada:issues.${issue.code}`, {
                    account: issue.accountId ? names.account(issue.accountId) : "",
                    currency: issue.currency ?? "",
                    date: issue.date ? formatCalendarDate(issue.date) : "",
                    detail: issue.detail ?? "",
                  })}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}

function RatesCard({ report }: { report: TaxReport }) {
  const { t } = useTranslation();
  const { formatCalendarDate } = useDateFormatting();
  if (report.fxFallbacks.length === 0 && report.rateFailures.length === 0) return null;
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">{t("canada:fx.title")}</CardTitle>
      </CardHeader>
      <CardContent className="grid gap-3 text-sm">
        {report.rateFailures.map((failure) => (
          <Alert
            key={`${failure.currency}-${failure.from}`}
            variant="warning"
            className="px-3 py-2.5"
          >
            <Icons.AlertTriangle className="h-4 w-4" />
            <AlertDescription className="text-xs">
              {t("canada:fx.failure", {
                currency: failure.currency,
                from: formatCalendarDate(failure.from),
                to: formatCalendarDate(failure.to),
              })}
            </AlertDescription>
          </Alert>
        ))}
        {report.fxFallbacks.length > 0 && (
          <div>
            <p>{t("canada:fx.fallbacks", { count: report.fxFallbacks.length })}</p>
            <ul className="text-muted-foreground mt-1 list-disc ps-5 text-xs">
              {report.fxFallbacks.map((fallback) => (
                <li key={fallback.activityId}>
                  {t("canada:fx.fallback_row", {
                    currency: fallback.currency,
                    date: formatCalendarDate(fallback.date),
                    rateDate: formatCalendarDate(fallback.rateDate),
                  })}
                </li>
              ))}
            </ul>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function ReportBody({ report }: { report: TaxReport }) {
  const { t } = useTranslation();
  const names = useSecurityNames();
  const nothing =
    report.holders.length === 0 && report.issues.length === 0 && report.rateFailures.length === 0;

  return (
    <div className="grid gap-6">
      {report.unreadableAccountIds.length > 0 && (
        <Alert variant="warning" className="px-3 py-2.5">
          <Icons.AlertTriangle className="h-4 w-4" />
          <AlertDescription className="text-xs">
            {t("canada:unreadable_accounts", {
              accounts: report.unreadableAccountIds.map(names.account).join(", "),
            })}
          </AlertDescription>
        </Alert>
      )}

      {nothing ? (
        <Empty>
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <Icons.FileText />
            </EmptyMedia>
            <EmptyTitle>{t("canada:empty_title")}</EmptyTitle>
            <EmptyDescription>{t("canada:empty_description")}</EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <Button render={<Link to="/settings/accounts" />}>{t("canada:empty_action")}</Button>
          </EmptyContent>
        </Empty>
      ) : (
        <>
          {report.holders.map((holder) => (
            <HolderSection
              key={holder.holder}
              holder={holder}
              showName={report.holders.length > 1}
            />
          ))}
          <IssuesCard issues={report.issues} />
          <RatesCard report={report} />
        </>
      )}
    </div>
  );
}

type TaxView = "gains" | "room";

export default function TaxReportPage() {
  const { t } = useTranslation();
  const [searchParams, setSearchParams] = useSearchParams();
  const view: TaxView = searchParams.get("view") === "room" ? "room" : "gains";
  const setView = (next: TaxView) =>
    setSearchParams(
      (params) => {
        if (next === "room") params.set("view", "room");
        else params.delete("view");
        return params;
      },
      { replace: true },
    );

  return (
    <Page>
      <PageHeader
        heading={t("canada:title")}
        text={view === "room" ? t("canada:room.subtitle") : t("canada:subtitle")}
        actions={
          <AnimatedToggleGroup<TaxView>
            value={view}
            rounded="lg"
            size="sm"
            onValueChange={(next) => {
              if (next === "gains" || next === "room") setView(next);
            }}
            items={[
              { value: "gains", label: t("canada:view_gains") },
              { value: "room", label: t("canada:view_room") },
            ]}
          />
        }
      />
      <PageContent>
        <div className="grid gap-6">
          <Alert className="px-3 py-2.5">
            <Icons.Info className="h-4 w-4" />
            <AlertDescription className="text-xs">{t("canada:disclaimer")}</AlertDescription>
          </Alert>
          {view === "room" ? <ContributionRoomView /> : <GainsView />}
        </div>
      </PageContent>
    </Page>
  );
}

function GainsView() {
  const { t } = useTranslation();
  const { formatCalendarDate } = useDateFormatting();
  const { data: report, isLoading, isError } = useTaxReport();

  if (isLoading) {
    return (
      <div className="grid gap-4">
        <Skeleton className="h-48 w-full rounded-xl" />
        <Skeleton className="h-48 w-full rounded-xl" />
      </div>
    );
  }
  if (isError || !report) {
    return (
      <Alert variant="destructive">
        <Icons.AlertTriangle className="h-4 w-4" />
        <AlertDescription>{t("canada:error")}</AlertDescription>
      </Alert>
    );
  }
  return (
    <div className="grid gap-6">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <p className="text-muted-foreground text-sm">
          {t("canada:as_of", { date: formatCalendarDate(report.asOf) })}
        </p>
        {report.holders.some((holder) => holder.years.length > 0) && (
          <Button variant="outline" size="sm" render={<Link to="/tax/statement" />}>
            <Icons.FileText className="me-2 h-4 w-4" />
            {t("canada:statement.open")}
          </Button>
        )}
      </div>
      <ReportBody report={report} />
    </div>
  );
}
