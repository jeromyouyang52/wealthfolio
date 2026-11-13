// The Questrade and Interactive Brokers system templates, read from the
// migration that ships them, applied to export rows by the importer's own
// draft builder. Rows follow the brokers' published export columns; amounts
// are worked by hand in the comments.
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import { createDraftActivities, validateDraft } from "@/pages/activity/import/utils/draft-utils";

const migration = readFileSync(
  path.resolve(
    path.dirname(fileURLToPath(import.meta.url)),
    "../../../../../crates/storage-sqlite/migrations/2026-11-13-000001_loontax_import_templates/up.sql",
  ),
  "utf8",
);

interface TemplateConfig {
  fieldMappings: Record<string, string>;
  activityMappings: Record<string, string[]>;
  symbolMappings: Record<string, string>;
  accountMappings: Record<string, string>;
  parseConfig: { dateFormat: string; decimalSeparator: string; thousandsSeparator: string };
}

function template(id: string): TemplateConfig {
  // VALUES ('<id>', '<name>', 'SYSTEM', 'CSV_ACTIVITY', '', 1, '<config>', ...)
  const match = new RegExp(`'${id}',[\\s\\S]*?\\n\\s*1,\\n\\s*'(\\{[\\s\\S]*?\\})',`).exec(
    migration,
  );
  if (!match) throw new Error(`template ${id} not found in the migration`);
  return JSON.parse(match[1].replaceAll("''", "'")) as TemplateConfig;
}

/** Splits CSV text the way the export writes it: quoted fields may hold commas. */
function parseCsv(text: string): { headers: string[]; rows: string[][] } {
  const lines = text.trim().split("\n");
  const split = (line: string) =>
    [...line.matchAll(/("([^"]*(?:""[^"]*)*)"|[^,]*)(,|$)/g)]
      .slice(0, -1)
      .map((m) => (m[2] !== undefined ? m[2].replaceAll('""', '"') : m[1]));
  const [header, ...rest] = lines;
  return { headers: split(header), rows: rest.map(split) };
}

function drafts(id: string, csv: string) {
  const config = template(id);
  const { headers, rows } = parseCsv(csv);
  return createDraftActivities(
    rows,
    headers,
    config,
    { ...config.parseConfig, defaultCurrency: "CAD" },
    "account-1",
  ).map((draft) => ({ ...draft, ...validateDraft(draft) }));
}

describe("Questrade template", () => {
  const csv = `Transaction Date,Settlement Date,Action,Symbol,Description,Quantity,Price,Gross Amount,Commission,Net Amount,Currency,Account #,Activity Type,Account Type
2026-03-02 12:00:00 AM,2026-03-03 12:00:00 AM,Buy,XEQT.TO,"ISHARES CORE EQUITY ETF PORTFOLIO, WE ACTED AS AGENT",100,30.00,-3000.00,-4.95,-3004.95,CAD,12345678,Trades,Individual margin
2026-06-01 12:00:00 AM,2026-06-02 12:00:00 AM,Sell,XEQT.TO,ISHARES CORE EQUITY ETF PORTFOLIO WE ACTED AS AGENT,-60,35.00,2100.00,-4.95,2095.05,CAD,12345678,Trades,Individual margin
2026-03-27 12:00:00 AM,2026-03-27 12:00:00 AM,DIV,XEQT.TO,ISHARES CORE EQUITY ETF PORTFOLIO DIST ON 100 SHS,0,0.00,0.00,0.00,18.50,CAD,12345678,Dividends,Individual margin
2026-01-05 12:00:00 AM,2026-01-05 12:00:00 AM,DEP,,ELECTRONIC FUNDS TRANSFER,0,0.00,0.00,0.00,5000.00,CAD,12345678,Deposits,Individual margin
2026-02-10 12:00:00 AM,2026-02-10 12:00:00 AM,XYZ,,AN ACTIVITY THE TEMPLATE DOES NOT KNOW,0,0.00,0.00,0.00,1.00,CAD,12345678,Other,Individual margin`;

  it("reads trades with positive units, final amounts and the trade date", () => {
    const [buy, sell] = drafts("system_questrade", csv);

    expect(buy.activityType).toBe("BUY");
    expect(buy.symbol).toBe("XEQT.TO");
    expect(buy.activityDate.slice(0, 10)).toBe("2026-03-02");
    expect(Number(buy.quantity)).toBe(100);
    expect(Number(buy.unitPrice)).toBe(30);
    // 100 x 30.00 + 4.95 commission
    expect(Math.abs(Number(buy.amount))).toBe(3004.95);
    expect(Math.abs(Number(buy.fee))).toBe(4.95);
    expect(buy.currency).toBe("CAD");
    expect(buy.comment).toContain("WE ACTED AS AGENT");
    expect(buy.status).not.toBe("error");

    expect(sell.activityType).toBe("SELL");
    expect(Number(sell.quantity)).toBe(60);
    // 60 x 35.00 - 4.95 commission
    expect(Math.abs(Number(sell.amount))).toBe(2095.05);
    expect(sell.status).not.toBe("error");
  });

  it("reads dividends and deposits", () => {
    const [, , dividend, deposit] = drafts("system_questrade", csv);
    expect(dividend.activityType).toBe("DIVIDEND");
    expect(Number(dividend.amount)).toBe(18.5);
    expect(deposit.activityType).toBe("DEPOSIT");
    expect(Number(deposit.amount)).toBe(5000);
  });

  it("leaves an unknown code for the user to map instead of guessing", () => {
    const unknown = drafts("system_questrade", csv)[4];
    expect(unknown.activityType).toBeFalsy();
    expect(unknown.status).toBe("error");
  });
});

describe("Interactive Brokers Flex Query template", () => {
  const csv = `"ClientAccountID","CurrencyPrimary","Symbol","Description","ISIN","TradeDate","Quantity","TradePrice","IBCommission","NetCash","Buy/Sell"
"U1234567","USD","VTI","VANGUARD TOTAL STOCK MKT ETF","US9229087690","2026-06-29","10","100","-1","-1001","BUY"
"U1234567","USD","VTI","VANGUARD TOTAL STOCK MKT ETF","US9229087690","2026-07-01","-10","110","-1","1099","SELL"`;

  it("reads buys and sells in the trade currency", () => {
    const [buy, sell] = drafts("system_ibkr_flex_trades", csv);

    expect(buy.activityType).toBe("BUY");
    expect(buy.symbol).toBe("VTI");
    expect(buy.isin).toBe("US9229087690");
    expect(buy.activityDate.slice(0, 10)).toBe("2026-06-29");
    expect(Number(buy.quantity)).toBe(10);
    // 10 x 100 + 1 commission
    expect(Math.abs(Number(buy.amount))).toBe(1001);
    expect(buy.currency).toBe("USD");
    expect(buy.status).not.toBe("error");

    expect(sell.activityType).toBe("SELL");
    expect(Number(sell.quantity)).toBe(10);
    // 10 x 110 - 1 commission
    expect(Math.abs(Number(sell.amount))).toBe(1099);
    expect(sell.status).not.toBe("error");
  });

  // IBKR's default Flex date format is yyyyMMdd.
  it("reads IBKR's default compact dates", () => {
    const compact = csv.replace('"2026-06-29"', '"20260629"');
    expect(drafts("system_ibkr_flex_trades", compact)[0].activityDate.slice(0, 10)).toBe(
      "2026-06-29",
    );
  });
});
