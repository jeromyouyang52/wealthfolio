import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router-dom";

import { useAccounts } from "@/hooks/use-accounts";
import { useBalancePrivacy } from "@/hooks/use-balance-privacy";
import {
  Badge,
  Button,
  Skeleton,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
  useAmountFormatting,
  useDateFormatting,
} from "@wealthfolio/ui";
import { Alert, AlertDescription } from "@wealthfolio/ui/components/ui/alert";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@wealthfolio/ui/components/ui/card";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@wealthfolio/ui/components/ui/empty";
import { Icons } from "@wealthfolio/ui/components/ui/icons";

import { useContributionRoom } from "../hooks/use-contribution-room";
import type {
  FhsaRoom,
  HolderRoom,
  PlanResult,
  RoomIssue,
  RoomPlan,
  RoomReport,
  RrspRoom,
  TfsaRoom,
} from "../types/contribution-room";
import { RoomStartDialog } from "./room-start-dialog";

const CAD = "CAD";

function useCad() {
  const amountFormatting = useAmountFormatting();
  const { isBalanceHidden } = useBalancePrivacy();
  return (value: number) => (isBalanceHidden ? "••••" : amountFormatting.formatAmount(value, CAD));
}

function holderName(holder: string, t: (key: string) => string) {
  if (holder === "self") return t("canada:holder_self");
  if (holder === "spouse") return t("canada:holder_spouse");
  return holder;
}

/** One labelled figure in a plan card. */
function Line({ label, value, tip }: { label: string; value: string; tip?: string }) {
  return (
    <div className="flex items-baseline justify-between gap-3 text-sm">
      <dt className="text-muted-foreground">
        {tip ? (
          <Tooltip>
            <TooltipTrigger
              render={<span className="cursor-help underline decoration-dotted" tabIndex={0} />}
            >
              {label}
            </TooltipTrigger>
            <TooltipContent className="max-w-xs">{tip}</TooltipContent>
          </Tooltip>
        ) : (
          label
        )}
      </dt>
      <dd className="text-end tabular-nums">{value}</dd>
    </div>
  );
}

/** The headline: what can still go in, or how far over the room it is. */
function Headline({ label, amount }: { label: string; amount: number }) {
  const { t } = useTranslation();
  const cad = useCad();
  const over = amount < 0;
  return (
    <div className="grid gap-0.5">
      <span className="text-muted-foreground text-xs">
        {over ? t("canada:room.over_by") : label}
      </span>
      <span className={`text-2xl font-semibold tabular-nums ${over ? "text-destructive" : ""}`}>
        {cad(Math.abs(amount))}
      </span>
    </div>
  );
}

function TaxWarning({ tax, months }: { tax: number; months: number }) {
  const { t } = useTranslation();
  const cad = useCad();
  if (tax <= 0) return null;
  return (
    <Alert variant="warning" className="px-3 py-2">
      <Icons.AlertTriangle className="h-4 w-4" />
      <AlertDescription className="text-xs">
        {t("canada:room.tax_estimate", { amount: cad(tax), count: months })}
      </AlertDescription>
    </Alert>
  );
}

function TfsaBody({ room }: { room: TfsaRoom }) {
  const { t } = useTranslation();
  const cad = useCad();
  const months = room.years.reduce((n, year) => n + year.excessMonths.length, 0);
  return (
    <>
      <Headline
        label={t("canada:room.tfsa.available", { year: room.year })}
        amount={room.availableCad}
      />
      <dl className="grid gap-1.5">
        <Line
          label={t("canada:room.tfsa.room_jan1", { year: room.year })}
          value={cad(room.roomJan1Cad)}
        />
        <Line label={t("canada:room.contributions")} value={cad(room.contributionsCad)} />
        <Line
          label={t("canada:room.withdrawals")}
          value={cad(room.withdrawalsCad)}
          tip={t("canada:room.tfsa.withdrawals_tip")}
        />
        {room.nextYearRoomCad !== null && (
          <Line
            label={t("canada:room.tfsa.next_year", { year: room.year + 1 })}
            value={cad(room.nextYearRoomCad)}
          />
        )}
      </dl>
      <TaxWarning tax={room.estimatedTaxCad} months={months} />
      {room.flowsBeforeStart > 0 && (
        <p className="text-muted-foreground text-xs">
          {t("canada:room.tfsa.before_start", { count: room.flowsBeforeStart })}
        </p>
      )}
    </>
  );
}

function RrspBody({ room }: { room: RrspRoom }) {
  const { t } = useTranslation();
  const cad = useCad();
  const { formatCalendarDate } = useDateFormatting();
  return (
    <>
      {room.outdated && (
        <Alert variant="warning" className="px-3 py-2">
          <Icons.AlertTriangle className="h-4 w-4" />
          <AlertDescription className="text-xs">
            {t("canada:room.rrsp.outdated", { year: room.year + 1 })}
          </AlertDescription>
        </Alert>
      )}
      <Headline
        label={t("canada:room.rrsp.remaining", { year: room.year })}
        amount={room.remainingCad}
      />
      <dl className="grid gap-1.5">
        <Line
          label={t("canada:room.rrsp.available", { year: room.year })}
          value={cad(room.availableRoomCad)}
        />
        <Line
          label={t("canada:room.contributions")}
          value={cad(room.contributionsCad)}
          tip={t("canada:room.rrsp.window", {
            from: formatCalendarDate(room.windowStart),
            to: formatCalendarDate(room.windowEnd),
          })}
        />
        {room.nextYearDollarLimitCad !== null && (
          <Line
            label={t("canada:room.rrsp.next_limit", { year: room.year + 1 })}
            value={cad(room.nextYearDollarLimitCad)}
            tip={t("canada:room.rrsp.next_limit_tip")}
          />
        )}
      </dl>
      {room.remainingCad < 0 && room.estimatedTaxCad === 0 && (
        <p className="text-muted-foreground text-xs">
          {t("canada:room.rrsp.cushion", { amount: cad(room.cushionCad) })}
        </p>
      )}
      <TaxWarning tax={room.estimatedTaxCad} months={room.excessMonths.length} />
      {room.beforeWindowCad > 0 && (
        <p className="text-muted-foreground text-xs">
          {t("canada:room.rrsp.before_window", { amount: cad(room.beforeWindowCad) })}
        </p>
      )}
      {room.afterWindowCad > 0 && (
        <p className="text-muted-foreground text-xs">
          {t("canada:room.rrsp.after_window", { amount: cad(room.afterWindowCad) })}
        </p>
      )}
    </>
  );
}

function FhsaBody({ room }: { room: FhsaRoom }) {
  const { t } = useTranslation();
  const cad = useCad();
  return (
    <>
      {room.participationPeriodMayHaveEnded && (
        <Alert variant="warning" className="px-3 py-2">
          <Icons.AlertTriangle className="h-4 w-4" />
          <AlertDescription className="text-xs">
            {t("canada:room.fhsa.period_ended")}
          </AlertDescription>
        </Alert>
      )}
      <Headline
        label={t("canada:room.fhsa.remaining", { year: room.year })}
        amount={room.remainingCad}
      />
      <dl className="grid gap-1.5">
        <Line
          label={t("canada:room.fhsa.room", { year: room.year })}
          value={cad(room.roomCad)}
          tip={t("canada:room.fhsa.carryforward", { amount: cad(room.carryforwardCad) })}
        />
        <Line label={t("canada:room.contributions")} value={cad(room.contributionsCad)} />
        {room.rrspTransfersCad > 0 && (
          <Line label={t("canada:room.fhsa.rrsp_transfers")} value={cad(room.rrspTransfersCad)} />
        )}
        <Line label={t("canada:room.fhsa.lifetime_left")} value={cad(room.lifetimeRemainingCad)} />
        <Line
          label={t("canada:room.fhsa.next_year", { year: room.year + 1 })}
          value={cad(room.nextYearRoomCad)}
        />
      </dl>
      <TaxWarning
        tax={room.estimatedTaxCad}
        months={room.years.reduce((n, year) => n + year.excessMonths.length, 0)}
      />
    </>
  );
}

function PlanCard<T>({
  plan,
  holder,
  result,
  currentYear,
  children,
}: {
  plan: RoomPlan;
  holder: HolderRoom;
  result: PlanResult<T>;
  currentYear: number;
  children: (room: T) => ReactNode;
}) {
  const { t } = useTranslation();
  const [editing, setEditing] = useState(false);
  const key = plan.toLowerCase();
  const estimate =
    plan === "TFSA" && holder.profile.tfsa?.basis === "eligibleSince" && result.status === "ok";

  return (
    <Card className="flex flex-col">
      <CardHeader className="flex flex-row items-start justify-between gap-2 space-y-0">
        <div className="grid gap-1">
          <CardTitle className="flex items-center gap-2 text-base">
            {t(`canada:room.${key}.title`)}
            {estimate && <Badge variant="secondary">{t("canada:room.estimate")}</Badge>}
            {result.incomplete && (
              <Badge variant="warning-subtle">{t("canada:room.incomplete")}</Badge>
            )}
          </CardTitle>
          {!result.hasAccount && (
            <CardDescription className="text-xs">{t("canada:room.no_account")}</CardDescription>
          )}
        </div>
        {result.status !== "needsStart" && (
          <Button variant="ghost" size="sm" onClick={() => setEditing(true)}>
            {t("canada:room.edit")}
          </Button>
        )}
      </CardHeader>
      <CardContent className="grid flex-1 content-start gap-4">
        {result.status === "needsStart" && (
          <div className="grid gap-3">
            <p className="text-muted-foreground text-sm">{t(`canada:room.${key}.needs_start`)}</p>
            <Button className="justify-self-start" onClick={() => setEditing(true)}>
              {t("canada:room.enter_figure")}
            </Button>
          </div>
        )}
        {result.status === "error" && (
          <Alert variant="destructive" className="px-3 py-2">
            <Icons.AlertTriangle className="h-4 w-4" />
            <AlertDescription className="text-xs">
              {t("canada:room.error", { detail: result.detail ?? "" })}
            </AlertDescription>
          </Alert>
        )}
        {result.status === "ok" && result.room !== null && children(result.room)}
      </CardContent>
      <RoomStartDialog
        holder={holder.holder}
        plan={plan}
        profile={holder.profile}
        currentYear={currentYear}
        open={editing}
        onOpenChange={setEditing}
      />
    </Card>
  );
}

function HolderSection({
  holder,
  showName,
  currentYear,
}: {
  holder: HolderRoom;
  showName: boolean;
  currentYear: number;
}) {
  const { t } = useTranslation();
  return (
    <section className="grid gap-4">
      {showName && <h2 className="text-lg font-semibold">{holderName(holder.holder, t)}</h2>}
      <div className="grid gap-4 lg:grid-cols-3">
        {holder.tfsa && (
          <PlanCard plan="TFSA" holder={holder} result={holder.tfsa} currentYear={currentYear}>
            {(room) => <TfsaBody room={room} />}
          </PlanCard>
        )}
        {holder.rrsp && (
          <PlanCard plan="RRSP" holder={holder} result={holder.rrsp} currentYear={currentYear}>
            {(room) => <RrspBody room={room} />}
          </PlanCard>
        )}
        {holder.fhsa && (
          <PlanCard plan="FHSA" holder={holder} result={holder.fhsa} currentYear={currentYear}>
            {(room) => <FhsaBody room={room} />}
          </PlanCard>
        )}
      </div>
    </section>
  );
}

function IssuesCard({ issues, showHolder }: { issues: RoomIssue[]; showHolder: boolean }) {
  const { t } = useTranslation();
  const { formatCalendarDate } = useDateFormatting();
  const amountFormatting = useAmountFormatting();
  const { isBalanceHidden } = useBalancePrivacy();
  const { accounts } = useAccounts({ filterActive: false, includeArchived: true });
  if (issues.length === 0) return null;
  const accountName = (id: string | null) => accounts.find((a) => a.id === id)?.name ?? "";
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">{t("canada:room.issues.title")}</CardTitle>
        <CardDescription>{t("canada:room.issues.description")}</CardDescription>
      </CardHeader>
      <CardContent>
        <ul className="grid gap-2 text-sm">
          {issues.map((issue, index) => (
            <li key={`${issue.activityId}-${issue.code}-${index}`} className="grid gap-0.5">
              <span>{t(`canada:room.issues.${issue.code}`)}</span>
              <span className="text-muted-foreground text-xs">
                {[
                  showHolder ? holderName(issue.holder, t) : null,
                  issue.plan,
                  accountName(issue.accountId),
                  issue.date ? formatCalendarDate(issue.date) : null,
                  issue.amount !== null && issue.currency
                    ? isBalanceHidden
                      ? "••••"
                      : amountFormatting.formatAmount(issue.amount, issue.currency)
                    : null,
                ]
                  .filter(Boolean)
                  .join(" · ")}
              </span>
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  );
}

function RoomBody({ report }: { report: RoomReport }) {
  const { t } = useTranslation();
  const { formatCalendarDate } = useDateFormatting();
  const currentYear = Number(report.asOf.slice(0, 4));

  if (report.holders.length === 0) {
    return (
      <Empty>
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <Icons.Wallet />
          </EmptyMedia>
          <EmptyTitle>{t("canada:room.empty_title")}</EmptyTitle>
          <EmptyDescription>{t("canada:room.empty_description")}</EmptyDescription>
        </EmptyHeader>
        <EmptyContent>
          <Button render={<Link to="/settings/accounts" />}>{t("canada:empty_action")}</Button>
        </EmptyContent>
      </Empty>
    );
  }

  return (
    <div className="grid gap-6">
      {report.unreadableProfiles && (
        <Alert variant="warning" className="px-3 py-2.5">
          <Icons.AlertTriangle className="h-4 w-4" />
          <AlertDescription className="text-xs">
            {t("canada:room.unreadable_profiles")}
          </AlertDescription>
        </Alert>
      )}
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
      {report.holders.map((holder) => (
        <HolderSection
          key={holder.holder}
          holder={holder}
          showName={report.holders.length > 1}
          currentYear={currentYear}
        />
      ))}
      <IssuesCard issues={report.issues} showHolder={report.holders.length > 1} />
      <p className="text-muted-foreground text-xs">{t("canada:room.footnote")}</p>
    </div>
  );
}

/** The contribution room tab of the tax page. */
export function ContributionRoomView() {
  const { t } = useTranslation();
  const { data: report, isLoading, isError } = useContributionRoom();
  if (isLoading) {
    return (
      <div className="grid gap-4 lg:grid-cols-3">
        <Skeleton className="h-64 w-full rounded-xl" />
        <Skeleton className="h-64 w-full rounded-xl" />
        <Skeleton className="h-64 w-full rounded-xl" />
      </div>
    );
  }
  if (isError || !report) {
    return (
      <Alert variant="destructive">
        <Icons.AlertTriangle className="h-4 w-4" />
        <AlertDescription>{t("canada:room.load_error")}</AlertDescription>
      </Alert>
    );
  }
  return <RoomBody report={report} />;
}
