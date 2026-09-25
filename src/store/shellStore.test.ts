import { describe, expect, it, vi } from "vitest";
import { createShellStore } from "./shellStore";
import { createMockShellIpc } from "../test/mockShellIpc";
import { service, snapshot } from "../test/fixtures";

describe("createShellStore", () => {
  it("starts loading, then becomes ready with the fetched snapshot", async () => {
    const ipc = createMockShellIpc(snapshot());
    const store = createShellStore(ipc);

    expect(store.getState()).toEqual({ status: "loading" });

    store.start();
    await vi.waitFor(() => {
      expect(store.getState().status).toBe("ready");
    });

    expect(ipc.getSnapshot).toHaveBeenCalledTimes(1);
    const state = store.getState();
    if (state.status !== "ready") throw new Error("expected ready state");
    expect(state.snapshot.services).toHaveLength(2);
    expect(state.selectedId).toBeNull();
  });

  it("initializes selectedId from snapshot.activeServiceId", async () => {
    const ipc = createMockShellIpc(snapshot({ activeServiceId: "icloud" }));
    const store = createShellStore(ipc);

    store.start();
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));

    const state = store.getState();
    if (state.status !== "ready") throw new Error("expected ready state");
    expect(state.selectedId).toBe("icloud");
  });

  it("re-syncs selectedId to activeServiceId on every refetch, overriding the previous local selection", async () => {
    const ipc = createMockShellIpc(snapshot({ activeServiceId: "gmail" }));
    const store = createShellStore(ipc);
    store.start();
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));

    store.select("icloud");
    const afterSelect = store.getState();
    if (afterSelect.status !== "ready") throw new Error("expected ready state");
    expect(afterSelect.selectedId).toBe("icloud");

    ipc.getSnapshot.mockResolvedValueOnce(snapshot({ activeServiceId: "gmail" }));
    ipc.emitServicesChanged();

    await vi.waitFor(() => {
      const state = store.getState();
      if (state.status !== "ready") throw new Error("expected ready state");
      expect(state.selectedId).toBe("gmail");
    });
  });

  it("is idempotent: a second start() does not refetch", async () => {
    const ipc = createMockShellIpc(snapshot());
    const store = createShellStore(ipc);

    store.start();
    store.start();
    await vi.waitFor(() => {
      expect(store.getState().status).toBe("ready");
    });

    expect(ipc.getSnapshot).toHaveBeenCalledTimes(1);
  });

  it("select() optimistically updates selectedId and calls selectService", async () => {
    const ipc = createMockShellIpc(snapshot());
    const store = createShellStore(ipc);
    store.start();
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));

    store.select("icloud");

    const state = store.getState();
    if (state.status !== "ready") throw new Error("expected ready state");
    expect(state.selectedId).toBe("icloud");
    expect(ipc.selectService).toHaveBeenCalledWith("icloud");
  });

  it("updates selectedId when a select-service event fires", async () => {
    const ipc = createMockShellIpc(snapshot());
    const store = createShellStore(ipc);
    store.start();
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));

    ipc.emitSelectService("icloud");

    const state = store.getState();
    if (state.status !== "ready") throw new Error("expected ready state");
    expect(state.selectedId).toBe("icloud");
  });

  it("refetches the snapshot when a services-changed event fires", async () => {
    const ipc = createMockShellIpc(snapshot());
    const store = createShellStore(ipc);
    store.start();
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));

    ipc.getSnapshot.mockResolvedValueOnce(snapshot({ services: [service({ id: "outlook" })] }));
    ipc.emitServicesChanged();

    await vi.waitFor(() => {
      const state = store.getState();
      if (state.status !== "ready") throw new Error("expected ready state");
      expect(state.snapshot.services).toHaveLength(1);
    });
    expect(ipc.getSnapshot).toHaveBeenCalledTimes(2);
  });

  it("reorder() optimistically reorders, then refetches on failure", async () => {
    const ipc = createMockShellIpc(snapshot());
    const store = createShellStore(ipc);
    store.start();
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));

    ipc.reorderServices.mockRejectedValueOnce(new Error("nope"));
    ipc.getSnapshot.mockResolvedValueOnce(snapshot());

    store.reorder(["icloud", "gmail"]);

    const optimistic = store.getState();
    if (optimistic.status !== "ready") throw new Error("expected ready state");
    expect(optimistic.snapshot.services.map((s) => s.id)).toEqual(["icloud", "gmail"]);

    await vi.waitFor(() => {
      expect(ipc.getSnapshot).toHaveBeenCalledTimes(2);
    });
  });

  it("select() and reorder() do nothing when the snapshot has a configError", async () => {
    const errorSnapshot = snapshot({
      services: [],
      settings: null,
      configError: { file: "/tmp/config.toml", key: null, reason: "invalid TOML" },
    });
    const ipc = createMockShellIpc(errorSnapshot);
    const store = createShellStore(ipc);
    store.start();
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));

    store.select("gmail");
    store.reorder(["gmail"]);

    expect(ipc.selectService).not.toHaveBeenCalled();
    expect(ipc.reorderServices).not.toHaveBeenCalled();
  });

  it("stop() unsubscribes from events", async () => {
    const ipc = createMockShellIpc(snapshot());
    const store = createShellStore(ipc);
    store.start();
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));

    store.stop();
    ipc.emitSelectService("icloud");

    const state = store.getState();
    if (state.status !== "ready") throw new Error("expected ready state");
    expect(state.selectedId).toBeNull();
  });

  it("becomes an error state when getSnapshot rejects", async () => {
    const ipc = createMockShellIpc(snapshot());
    ipc.getSnapshot.mockRejectedValueOnce(new Error("boom"));
    const store = createShellStore(ipc);

    store.start();
    await vi.waitFor(() => {
      expect(store.getState().status).toBe("error");
    });

    const state = store.getState();
    if (state.status !== "error") throw new Error("expected error state");
    expect(state.message).toBe("boom");
  });
});
