import { useMemo } from "react";
import { useTranslation } from "react-i18next";

import {
  AnimatedToggleGroup,
  ResponsiveSelect,
  type ResponsiveSelectOption,
} from "@wealthfolio/ui";
import { Alert, AlertDescription } from "@wealthfolio/ui/components/ui/alert";
import { Button } from "@wealthfolio/ui/components/ui/button";
import { Icons } from "@wealthfolio/ui/components/ui/icons";

import { cn } from "@/lib/utils";

import {
  DEFAULT_HOLDER,
  HOLDERS,
  REGISTRATIONS,
  getCanadaInfo,
  setCanadaInfo,
  suggestRegistration,
  type Registration,
} from "../lib/registration";

const UNSET = "__unset__";

interface CanadaRegistrationFieldProps {
  /** The account's `meta` as the form holds it. */
  meta?: string | null;
  onMetaChange: (meta: string) => void;
}

/**
 * How the account is registered with the CRA and whose it is. The Canadian
 * tax report reads both from the account's `meta`; an account left unset is
 * kept out of the report.
 */
export function CanadaRegistrationField({ meta, onMetaChange }: CanadaRegistrationFieldProps) {
  const { t } = useTranslation();
  const state = getCanadaInfo(meta);
  const info = state === "unreadable" ? null : state;
  const suggestion = info ? null : suggestRegistration(meta);

  const registrationLabel = (code: Registration) =>
    t(`settings:accounts.form_canada_registration_${code.toLowerCase()}`);

  const options: ResponsiveSelectOption[] = useMemo(
    () => [
      { label: t("settings:accounts.form_canada_registration_unset"), value: UNSET },
      ...REGISTRATIONS.map((code) => ({
        label: t(`settings:accounts.form_canada_registration_${code.toLowerCase()}`),
        value: code,
      })),
    ],
    [t],
  );

  const holderItems = useMemo(() => {
    const items: { value: string; label: string }[] = HOLDERS.map((holder) => ({
      value: holder,
      label: t(`settings:accounts.form_canada_holder_${holder}`),
    }));
    // A holder written by hand elsewhere stays selectable as it is.
    if (info && !items.some((item) => item.value === info.holder)) {
      items.push({ value: info.holder, label: info.holder });
    }
    return items;
  }, [info, t]);

  const setRegistration = (value: string) => {
    onMetaChange(
      setCanadaInfo(
        meta,
        value === UNSET
          ? null
          : { registration: value as Registration, holder: info?.holder ?? DEFAULT_HOLDER },
      ),
    );
  };

  return (
    <div className="grid gap-4">
      <div className="flex flex-col gap-2">
        <div>
          <label className="text-sm font-medium">
            {t("settings:accounts.form_canada_registration_label")}
          </label>
          <p className="text-muted-foreground text-xs">
            {t("settings:accounts.form_canada_registration_description")}
          </p>
        </div>
        <ResponsiveSelect
          aria-label={t("settings:accounts.form_canada_registration_label")}
          value={info?.registration ?? UNSET}
          onValueChange={setRegistration}
          options={options}
          placeholder={t("settings:accounts.form_canada_registration_label")}
          sheetTitle={t("settings:accounts.form_canada_registration_label")}
          sheetDescription={t("settings:accounts.form_canada_registration_description")}
          triggerClassName="h-11"
        />
        {state === "unreadable" && (
          <Alert variant="warning" className="px-3 py-2.5">
            <Icons.AlertTriangle className="h-4 w-4" />
            <AlertDescription className="text-xs">
              {t("settings:accounts.form_canada_unreadable")}
            </AlertDescription>
          </Alert>
        )}
        {suggestion && (
          <div className="text-muted-foreground flex flex-wrap items-center gap-2 text-xs">
            <Icons.Info className="h-3.5 w-3.5 shrink-0" />
            <span>
              {t("settings:accounts.form_canada_suggestion", {
                registration: registrationLabel(suggestion),
              })}
            </span>
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-7"
              onClick={() => setRegistration(suggestion)}
            >
              {t("settings:accounts.form_canada_use_suggestion")}
            </Button>
          </div>
        )}
      </div>

      {info && (
        <div className="flex flex-col gap-2">
          <div>
            <label className="text-sm font-medium">
              {t("settings:accounts.form_canada_holder_label")}
            </label>
            <p className="text-muted-foreground text-xs">
              {t("settings:accounts.form_canada_holder_description")}
            </p>
          </div>
          <AnimatedToggleGroup
            size="md"
            aria-label={t("settings:accounts.form_canada_holder_label")}
            value={info.holder}
            onValueChange={(holder) => onMetaChange(setCanadaInfo(meta, { ...info, holder }))}
            className={cn("grid w-full", holderItems.length > 2 ? "grid-cols-3" : "grid-cols-2")}
            items={holderItems}
          />
        </div>
      )}
    </div>
  );
}
