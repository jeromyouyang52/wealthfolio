import { zodResolver } from "@hookform/resolvers/zod";
import { useForm, type Resolver } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import * as z from "zod";

import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Form,
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  MoneyInput,
  RadioGroup,
  RadioGroupItem,
  Spinner,
} from "@wealthfolio/ui";

import { useSaveHolderProfile } from "../hooks/use-contribution-room";
import type { HolderProfile, RoomPlan } from "../types/contribution-room";

interface RoomStartValues {
  basis: "craRoom" | "eligibleSince";
  year?: number;
  amount?: number;
}

/** The earliest year each plan's starting figure can be for. */
const FIRST_YEAR: Record<RoomPlan, number> = { TFSA: 2009, RRSP: 1991, FHSA: 2023 };

function schemaFor(plan: RoomPlan, currentYear: number, required: string, range: string) {
  return z
    .object({
      basis: z.enum(["craRoom", "eligibleSince"]),
      year: z.number({ error: required }).int(),
      amount: z.number().optional(),
    })
    .superRefine((values, ctx) => {
      const earliest =
        plan === "TFSA" && values.basis === "eligibleSince" ? 1900 : FIRST_YEAR[plan];
      const latest = plan === "RRSP" ? currentYear + 1 : currentYear;
      if (values.year < earliest || values.year > latest) {
        ctx.addIssue({ code: "custom", path: ["year"], message: range });
      }
      const needsAmount = plan === "RRSP" || (plan === "TFSA" && values.basis === "craRoom");
      if (needsAmount && values.amount === undefined) {
        ctx.addIssue({ code: "custom", path: ["amount"], message: required });
      }
    });
}

function initialValues(plan: RoomPlan, profile: HolderProfile, currentYear: number) {
  switch (plan) {
    case "TFSA":
      return profile.tfsa?.basis === "craRoom"
        ? { basis: "craRoom" as const, year: profile.tfsa.year, amount: Number(profile.tfsa.room) }
        : profile.tfsa?.basis === "eligibleSince"
          ? { basis: "eligibleSince" as const, year: profile.tfsa.year }
          : { basis: "craRoom" as const, year: currentYear };
    case "RRSP":
      return profile.rrsp
        ? {
            basis: "craRoom" as const,
            year: profile.rrsp.year,
            amount: Number(profile.rrsp.availableRoom),
          }
        : { basis: "craRoom" as const, year: currentYear };
    case "FHSA":
      return { basis: "craRoom" as const, year: profile.fhsa?.openedYear };
  }
}

/** The holder's profile with this plan's figure replaced (or removed). */
function withStart(
  plan: RoomPlan,
  profile: HolderProfile,
  values: RoomStartValues | null,
): HolderProfile {
  const next: HolderProfile = { ...profile };
  const year = values?.year ?? 0;
  // Amounts go as text so no binary rounding reaches the backend.
  const amount = String(values?.amount ?? 0);
  if (plan === "TFSA") {
    next.tfsa = !values
      ? undefined
      : values.basis === "craRoom"
        ? { basis: "craRoom", year, room: amount }
        : { basis: "eligibleSince", year };
  } else if (plan === "RRSP") {
    next.rrsp = values ? { year, availableRoom: amount } : undefined;
  } else {
    next.fhsa = values ? { openedYear: year } : undefined;
  }
  for (const key of ["tfsa", "rrsp", "fhsa"] as const) {
    if (next[key] === undefined) delete next[key];
  }
  return next;
}

interface RoomStartDialogProps {
  holder: string;
  plan: RoomPlan;
  profile: HolderProfile;
  currentYear: number;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function RoomStartDialog(props: RoomStartDialogProps) {
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        {/* Mounted only while open, so each opening starts from the saved figure. */}
        {props.open && <RoomStartForm {...props} />}
      </DialogContent>
    </Dialog>
  );
}

function RoomStartForm({ holder, plan, profile, currentYear, onOpenChange }: RoomStartDialogProps) {
  const { t } = useTranslation();
  const save = useSaveHolderProfile();
  const form = useForm<RoomStartValues>({
    // The schema only makes `year` required; the values keep this shape.
    resolver: zodResolver(
      schemaFor(
        plan,
        currentYear,
        t("canada:room.form.required"),
        t("canada:room.form.year_range"),
      ),
    ) as unknown as Resolver<RoomStartValues>,
    defaultValues: initialValues(plan, profile, currentYear),
  });
  const basis = form.watch("basis");
  const key = plan.toLowerCase() as "tfsa" | "rrsp" | "fhsa";
  const hasSaved = profile[key] !== undefined;

  const submit = async (values: RoomStartValues | null) => {
    try {
      await save.mutateAsync({ holder, profile: withStart(plan, profile, values) });
      onOpenChange(false);
    } catch {
      toast.error(t("canada:room.form.save_failed"));
    }
  };

  const yearLabel =
    plan === "FHSA"
      ? t("canada:room.form.fhsa_year")
      : plan === "RRSP"
        ? t("canada:room.form.rrsp_year")
        : basis === "craRoom"
          ? t("canada:room.form.tfsa_cra_year")
          : t("canada:room.form.tfsa_eligible_year");

  return (
    <Form {...form}>
      <form onSubmit={form.handleSubmit((values) => submit(values))} className="space-y-5">
        <DialogHeader>
          <DialogTitle>{t(`canada:room.form.title_${key}`)}</DialogTitle>
          <DialogDescription>{t(`canada:room.form.where_${key}`)}</DialogDescription>
        </DialogHeader>

        {plan === "TFSA" && (
          <FormField
            control={form.control}
            name="basis"
            render={({ field }) => (
              <FormItem>
                <FormControl>
                  <RadioGroup
                    value={field.value}
                    onValueChange={(value) => field.onChange(value)}
                    className="grid gap-2"
                  >
                    {(["craRoom", "eligibleSince"] as const).map((option) => (
                      <label
                        key={option}
                        className={`hover:bg-accent flex cursor-pointer gap-3 rounded-lg border p-3 ${
                          field.value === option ? "border-primary bg-primary/5" : "border-muted"
                        }`}
                      >
                        <RadioGroupItem value={option} className="mt-0.5" />
                        <span className="flex flex-col">
                          <span className="text-sm font-medium">
                            {t(`canada:room.form.basis_${option}`)}
                          </span>
                          <span className="text-muted-foreground text-xs">
                            {t(`canada:room.form.basis_${option}_hint`)}
                          </span>
                        </span>
                      </label>
                    ))}
                  </RadioGroup>
                </FormControl>
              </FormItem>
            )}
          />
        )}

        <div className="grid gap-4 sm:grid-cols-[8rem_minmax(0,1fr)]">
          <FormField
            control={form.control}
            name="year"
            render={({ field }) => (
              <FormItem>
                <FormLabel>{yearLabel}</FormLabel>
                <FormControl>
                  <MoneyInput
                    className="text-start"
                    maxDecimalPlaces={0}
                    value={field.value}
                    onValueChange={(value) => field.onChange(value)}
                  />
                </FormControl>
                <FormMessage />
              </FormItem>
            )}
          />
          {(plan === "RRSP" || (plan === "TFSA" && basis === "craRoom")) && (
            <FormField
              control={form.control}
              name="amount"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>
                    {plan === "RRSP"
                      ? t("canada:room.form.rrsp_amount")
                      : t("canada:room.form.tfsa_amount")}
                  </FormLabel>
                  <FormControl>
                    <MoneyInput
                      className="text-start"
                      thousandSeparator
                      value={field.value}
                      onValueChange={(value) => field.onChange(value)}
                    />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
          )}
        </div>
        <FormDescription>{t("canada:room.form.note")}</FormDescription>

        <DialogFooter className="gap-2">
          {hasSaved && (
            <Button
              type="button"
              variant="ghost"
              className="sm:me-auto"
              disabled={save.isPending}
              onClick={() => submit(null)}
            >
              {t("canada:room.form.clear")}
            </Button>
          )}
          <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
            {t("canada:room.form.cancel")}
          </Button>
          <Button type="submit" disabled={save.isPending}>
            {save.isPending && <Spinner className="me-2 h-4 w-4" />}
            {t("canada:room.form.save")}
          </Button>
        </DialogFooter>
      </form>
    </Form>
  );
}
