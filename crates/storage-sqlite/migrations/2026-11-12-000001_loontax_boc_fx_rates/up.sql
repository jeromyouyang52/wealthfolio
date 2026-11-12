-- LoonTax: Bank of Canada daily exchange rates used by the Canadian tax
-- ledger. Kept apart from the app's quotes, which come from other sources:
-- tax figures state which published rate they used.
CREATE TABLE loontax_boc_fx_rates (
    currency TEXT NOT NULL,
    rate_date TEXT NOT NULL,
    cad_per_unit TEXT NOT NULL,
    PRIMARY KEY (currency, rate_date)
);

-- The range of days already asked about for each currency. A day inside it
-- with no row above is a day the Bank published no rate.
CREATE TABLE loontax_boc_fx_coverage (
    currency TEXT NOT NULL PRIMARY KEY,
    covered_from TEXT NOT NULL,
    covered_to TEXT NOT NULL
);
