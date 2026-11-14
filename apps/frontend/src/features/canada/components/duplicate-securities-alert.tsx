import { useTranslation } from "react-i18next";

import { Alert, AlertDescription } from "@wealthfolio/ui/components/ui/alert";
import { Icons } from "@wealthfolio/ui/components/ui/icons";

import { useSecurityNames } from "../hooks/use-security-names";
import { duplicateSecurityCodes } from "../lib/statement";
import type { HolderReport } from "../types/tax-report";

/** Warns when one code stands for several of the holder's securities. */
export function DuplicateSecuritiesAlert({ holder }: { holder: HolderReport }) {
  const { t } = useTranslation();
  const names = useSecurityNames();
  const codes = duplicateSecurityCodes(holder, names.security);
  if (codes.length === 0) return null;
  return (
    <Alert variant="warning" className="px-3 py-2.5">
      <Icons.AlertTriangle className="h-4 w-4" />
      <AlertDescription className="text-xs">
        {t("canada:duplicate_securities", { codes: codes.join(", ") })}
      </AlertDescription>
    </Alert>
  );
}
