import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@/test/render";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { saveHolderProfile } from "../adapters/contribution-room";
import type { HolderProfile, RoomPlan } from "../types/contribution-room";
import { RoomStartDialog } from "./room-start-dialog";

vi.mock("../adapters/contribution-room", () => ({
  saveHolderProfile: vi.fn(async (_holder: string, profile: HolderProfile) => profile),
  getContributionRoom: vi.fn(),
}));

const saved: HolderProfile = { rrsp: { year: 2026, availableRoom: "18500" } };

function open(plan: RoomPlan, profile: HolderProfile = saved) {
  const onOpenChange = vi.fn();
  render(
    <QueryClientProvider client={new QueryClient()}>
      <RoomStartDialog
        holder="self"
        plan={plan}
        profile={profile}
        currentYear={2026}
        open
        onOpenChange={onOpenChange}
      />
    </QueryClientProvider>,
  );
  return onOpenChange;
}

const fields = () => screen.getAllByRole("textbox");

describe("RoomStartDialog", () => {
  beforeEach(() => vi.mocked(saveHolderProfile).mockClear());

  it("saves the TFSA figure and keeps the holder's other plans", async () => {
    const onOpenChange = open("TFSA");
    fireEvent.change(fields()[1], { target: { value: "14000" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(saveHolderProfile).toHaveBeenCalledTimes(1));
    expect(saveHolderProfile).toHaveBeenCalledWith("self", {
      tfsa: { basis: "craRoom", year: 2026, room: "14000" },
      rrsp: saved.rrsp,
    });
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
  });

  it("asks for an amount before saving the CRA figure", async () => {
    open("TFSA");
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Required")).toBeInTheDocument();
    expect(saveHolderProfile).not.toHaveBeenCalled();
  });

  it("refuses an FHSA opened before FHSAs existed", async () => {
    open("FHSA");
    fireEvent.change(fields()[0], { target: { value: "2021" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(
      await screen.findByText("Enter a year this plan's figure can be for"),
    ).toBeInTheDocument();
    expect(saveHolderProfile).not.toHaveBeenCalled();
  });

  it("removes only this plan's figure", async () => {
    open("RRSP", { ...saved, fhsa: { openedYear: 2024 } });
    fireEvent.click(screen.getByRole("button", { name: "Remove figure" }));
    await waitFor(() =>
      expect(saveHolderProfile).toHaveBeenCalledWith("self", { fhsa: { openedYear: 2024 } }),
    );
  });
});
