import { describe, expect, it } from "vitest";

import { getCanadaInfo, setCanadaInfo, suggestRegistration } from "./registration";

describe("getCanadaInfo", () => {
  it("is unset without meta or without a canada entry", () => {
    expect(getCanadaInfo(null)).toBeNull();
    expect(getCanadaInfo("  ")).toBeNull();
    expect(getCanadaInfo('{"accounting":{"costBasisMethod":"WAC"}}')).toBeNull();
  });

  it("reads the registration and defaults the holder", () => {
    expect(getCanadaInfo('{"canada":{"registration":"TFSA"}}')).toEqual({
      registration: "TFSA",
      holder: "self",
    });
    expect(getCanadaInfo('{"canada":{"registration":"SPOUSAL_RRSP","holder":"spouse"}}')).toEqual({
      registration: "SPOUSAL_RRSP",
      holder: "spouse",
    });
  });

  // The same cases the Rust reader refuses.
  it("calls what the tax report refuses unreadable", () => {
    for (const meta of [
      "{not json",
      "[]",
      '{"canada":"TFSA"}',
      '{"canada":{"registration":"ROTH_IRA"}}',
      '{"canada":{"registration":"TFSA","holder":" "}}',
      '{"canada":{"registration":"TFSA","extra":1}}',
    ]) {
      expect(getCanadaInfo(meta), meta).toBe("unreadable");
    }
  });
});

describe("setCanadaInfo", () => {
  it("keeps every other entry", () => {
    const written = setCanadaInfo(
      '{"accounting":{"costBasisMethod":"WAC"},"broker_account_type":"MARGIN"}',
      { registration: "NON_REGISTERED", holder: "self" },
    );
    expect(JSON.parse(written)).toEqual({
      accounting: { costBasisMethod: "WAC" },
      broker_account_type: "MARGIN",
      canada: { registration: "NON_REGISTERED", holder: "self" },
    });
    expect(getCanadaInfo(written)).toEqual({ registration: "NON_REGISTERED", holder: "self" });
  });

  it("removes the entry when unset, and replaces an unreadable meta", () => {
    expect(JSON.parse(setCanadaInfo('{"canada":{"registration":"TFSA"},"a":1}', null))).toEqual({
      a: 1,
    });
    expect(
      JSON.parse(setCanadaInfo("{not json", { registration: "TFSA", holder: "self" })),
    ).toEqual({ canada: { registration: "TFSA", holder: "self" } });
  });
});

describe("suggestRegistration", () => {
  it("suggests the registered plan the broker reports", () => {
    expect(suggestRegistration('{"broker_account_type":"TFSA"}')).toBe("TFSA");
    expect(suggestRegistration('{"broker_account_type":"rsp"}')).toBe("RRSP");
    expect(suggestRegistration('{"broker_account_type":"Spousal RRSP"}')).toBe("SPOUSAL_RRSP");
  });

  it("suggests nothing for other or missing broker types", () => {
    expect(suggestRegistration('{"broker_account_type":"MARGIN"}')).toBeNull();
    expect(suggestRegistration('{"broker_account_type":"ROTH_IRA"}')).toBeNull();
    expect(suggestRegistration("{}")).toBeNull();
    expect(suggestRegistration(null)).toBeNull();
  });
});
