import { useQuery } from "@tanstack/react-query";
import { useMemo } from "react";

import { getAssets } from "@/adapters";
import { useAccounts } from "@/hooks/use-accounts";
import { QueryKeys } from "@/lib/query-keys";

/** Display names for the security and account ids the tax reports carry. */
export function useSecurityNames() {
  const { data: assets = [] } = useQuery({ queryKey: [QueryKeys.ASSETS], queryFn: getAssets });
  const { accounts } = useAccounts({ filterActive: false, includeArchived: true });
  return useMemo(() => {
    const securities = new Map(assets.map((a) => [a.id, a.displayCode ?? a.name ?? a.id]));
    const descriptions = new Map(assets.map((a) => [a.id, a.name ?? ""]));
    const accountNames = new Map(accounts.map((a) => [a.id, a.name]));
    return {
      security: (id: string) => securities.get(id) ?? id,
      /** The security's full name, when it differs from its code. */
      description: (id: string) => {
        const name = descriptions.get(id) ?? "";
        return name && name !== securities.get(id) ? name : "";
      },
      account: (id: string) => accountNames.get(id) ?? id,
    };
  }, [assets, accounts]);
}
