-- LoonTax: system import templates for two Canadian brokers' exports.
-- Same shape as the system templates of 2026-03-19-000001_import_templates.
-- The configs are tested by apps/frontend/src/features/canada/import-templates.test.ts,
-- which reads them from this file.

-- Questrade: Reports > Account activity > Export. One row per activity; the
-- trade date is "Transaction Date"; sells have negative quantities and buys
-- negative amounts. Codes not listed here are left for the user to map.
INSERT OR REPLACE INTO import_templates (id, name, scope, kind, source_system, config_version, config, created_at, updated_at)
VALUES (
    'system_questrade',
    'Questrade',
    'SYSTEM',
    'CSV_ACTIVITY',
    '',
    1,
    '{"fieldMappings":{"date":"Transaction Date","activityType":"Action","symbol":"Symbol","quantity":"Quantity","unitPrice":"Price","fee":"Commission","amount":"Net Amount","currency":"Currency","account":"Account #","comment":"Description"},"activityMappings":{"BUY":["Buy","REI"],"SELL":["Sell"],"DIVIDEND":["DIV"],"DEPOSIT":["DEP","CON"],"WITHDRAWAL":["WDR"],"TRANSFER_IN":["TFI"],"TRANSFER_OUT":["TFO"]},"symbolMappings":{},"accountMappings":{},"symbolMappingMeta":{},"parseConfig":{"delimiter":",","dateFormat":"YYYY-MM-DD hh:mm:ss A","decimalSeparator":".","thousandsSeparator":"auto","hasHeaderRow":true,"skipTopRows":0,"skipBottomRows":0}}',
    CURRENT_TIMESTAMP,
    CURRENT_TIMESTAMP
);

-- Interactive Brokers: a Flex Query of the Trades section in CSV, without
-- header and trailer records. Columns are matched by name, so their order in
-- the query does not matter.
INSERT OR REPLACE INTO import_templates (id, name, scope, kind, source_system, config_version, config, created_at, updated_at)
VALUES (
    'system_ibkr_flex_trades',
    'Interactive Brokers (Flex Query, Trades)',
    'SYSTEM',
    'CSV_ACTIVITY',
    '',
    1,
    '{"fieldMappings":{"date":"TradeDate","activityType":"Buy/Sell","symbol":"Symbol","isin":"ISIN","quantity":"Quantity","unitPrice":"TradePrice","fee":"IBCommission","amount":"NetCash","currency":"CurrencyPrimary","account":"ClientAccountID","comment":"Description"},"activityMappings":{"BUY":["BUY"],"SELL":["SELL"]},"symbolMappings":{},"accountMappings":{},"symbolMappingMeta":{},"parseConfig":{"delimiter":",","dateFormat":"auto","decimalSeparator":".","thousandsSeparator":"auto","hasHeaderRow":true,"skipTopRows":0,"skipBottomRows":0}}',
    CURRENT_TIMESTAMP,
    CURRENT_TIMESTAMP
);
