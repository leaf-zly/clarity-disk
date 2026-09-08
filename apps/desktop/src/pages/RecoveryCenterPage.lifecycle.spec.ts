import { defineComponent, h, KeepAlive, ref } from "vue";
import { enableAutoUnmount, flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import RecoveryCenterPage from "@/pages/RecoveryCenterPage.vue";
import {
  getExecutionQuarantineIndex,
  updateQuarantinePolicy,
} from "@/services/cleanup-execution-service";
import {
  executeQuarantineDeletion,
  prepareQuarantineDeletion,
  restoreQuarantineEntryTo,
} from "@/services/operations-service";
import type {
  QuarantineExecutionIndex,
  QuarantineRestoreResult,
} from "@/types/cleanup-execution";

vi.mock("@/services/cleanup-execution-service", () => ({
  getExecutionQuarantineIndex: vi.fn(),
  updateQuarantinePolicy: vi.fn(),
}));
vi.mock("@/services/operations-service", () => ({
  executeQuarantineDeletion: vi.fn(),
  prepareQuarantineDeletion: vi.fn(),
  restoreQuarantineEntryTo: vi.fn(),
}));
enableAutoUnmount(afterEach);

/** Creates display-only quarantine fixtures; tests never touch real files. */
function makeIndex(ids = ["one", "two"]): QuarantineExecutionIndex {
  return {
    indexId: "index",
    planId: "plan",
    createdAtUnixMs: 1,
    filesMoved: true,
    totalBytes: ids.length * 32,
    policy: { retentionDays: 30, maxBytes: 10 * 1024 ** 3 },
    entries: ids.map((entryId) => ({
      entryId,
      candidateId: entryId,
      ruleId: "temp",
      originalPath: `C:\\Temp\\${entryId}.tmp`,
      quarantinePath: null,
      bytes: 32,
      metadataDigest: "digest",
      status: "staged",
      movedAtUnixMs: 1,
      restoredAtUnixMs: null,
      expiresAtUnixMs: null,
      transferKind: "rename",
      integrityDigest: null,
      transactionPath: null,
    })),
  };
}

/** Exercises real cached-page activation through a visible navigation control. */
function mountCachedPage() {
  return mount(
    defineComponent({
      setup() {
        const visible = ref(true);
        return () =>
          h("div", [
            h(
              "button",
              {
                "aria-label": "切换页面",
                onClick: () => {
                  visible.value = !visible.value;
                },
              },
              "切换页面",
            ),
            h(KeepAlive, null, {
              default: () => (visible.value ? h(RecoveryCenterPage) : null),
            }),
          ]);
      },
    }),
  );
}

describe("RecoveryCenterPage lifecycle and execution", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(getExecutionQuarantineIndex).mockResolvedValue(makeIndex());
    vi.mocked(prepareQuarantineDeletion).mockResolvedValue({
      authorizationId: "authorization",
      entryIds: ["one"],
      selectionDigest: "digest",
      confirmationToken: "token",
      confirmationPhrase: "永久删除",
      expiresAtUnixMs: Date.now() + 120000,
    });
  });

  it("loads once initially and refreshes cached contents on return", async () => {
    const wrapper = mountCachedPage();
    await flushPromises();
    expect(getExecutionQuarantineIndex).toHaveBeenCalledOnce();
    expect(wrapper.text()).toContain("one.tmp");
    await wrapper.get('[aria-label="切换页面"]').trigger("click");
    vi.mocked(getExecutionQuarantineIndex).mockResolvedValueOnce(
      makeIndex(["new"]),
    );
    await wrapper.get('[aria-label="切换页面"]').trigger("click");
    await flushPromises();
    expect(getExecutionQuarantineIndex).toHaveBeenCalledTimes(2);
    expect(wrapper.text()).toContain("new.tmp");
    expect(wrapper.text()).not.toContain("one.tmp");
  });

  it("waits for sequential restores and retains failed selections", async () => {
    const wrapper = mount(RecoveryCenterPage);
    await flushPromises();
    for (const input of wrapper.findAll('input[type="checkbox"]'))
      await input.setValue(true);
    let finish!: (result: QuarantineRestoreResult) => void;
    vi.mocked(restoreQuarantineEntryTo)
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            finish = resolve;
          }),
      )
      .mockRejectedValueOnce(new Error("locked"));
    await wrapper.get("button.primary").trigger("click");
    expect(restoreQuarantineEntryTo).toHaveBeenCalledTimes(1);
    expect(
      wrapper.get('input[type="checkbox"]').attributes("disabled"),
    ).toBeDefined();
    expect(wrapper.get("select").attributes("disabled")).toBeDefined();
    await wrapper.get("button.primary").trigger("click");
    vi.mocked(getExecutionQuarantineIndex).mockResolvedValueOnce(
      makeIndex(["two"]),
    );
    finish({ entryId: "one", status: "restored", reason: "restored" });
    await flushPromises();
    expect(restoreQuarantineEntryTo).toHaveBeenCalledTimes(2);
    expect(restoreQuarantineEntryTo).toHaveBeenNthCalledWith(
      2,
      "two",
      "original",
    );
    expect(wrapper.text()).toContain("已恢复 1 项，未恢复 1 项。");
    expect(wrapper.get('[role="alert"]').text()).toContain("部分项目未恢复");
    expect(
      (wrapper.get('input[type="checkbox"]').element as HTMLInputElement)
        .checked,
    ).toBe(true);
    expect(
      wrapper.get("button.primary").attributes("disabled"),
    ).toBeUndefined();
  });

  it("treats resolved restore conflicts as unsuccessful", async () => {
    const wrapper = mount(RecoveryCenterPage);
    await flushPromises();
    await wrapper.get('input[type="checkbox"]').setValue(true);
    vi.mocked(restoreQuarantineEntryTo).mockResolvedValueOnce({
      entryId: "one",
      status: "restoreConflict",
      reason: "exists",
    });
    await wrapper.get("button.primary").trigger("click");
    await flushPromises();
    expect(wrapper.text()).toContain("已恢复 0 项，未恢复 1 项。");
    expect(
      (wrapper.get('input[type="checkbox"]').element as HTMLInputElement)
        .checked,
    ).toBe(true);
  });

  it("invalidates deletion confirmation and locks stale state after refresh failure", async () => {
    const wrapper = mount(RecoveryCenterPage);
    await flushPromises();
    await wrapper.get('input[type="checkbox"]').setValue(true);
    await wrapper.get("button.danger-outline").trigger("click");
    await flushPromises();
    expect(wrapper.text()).toContain("此操作无法恢复");
    vi.mocked(getExecutionQuarantineIndex).mockRejectedValueOnce(
      new Error("read failed"),
    );
    await wrapper.get("button.secondary").trigger("click");
    await flushPromises();
    expect(wrapper.text()).not.toContain("此操作无法恢复");
    expect(wrapper.get("button.primary").attributes("disabled")).toBeDefined();
    expect(
      wrapper.get("button.danger-outline").attributes("disabled"),
    ).toBeDefined();
    await wrapper.get("button.danger-outline").trigger("click");
    expect(prepareQuarantineDeletion).toHaveBeenCalledOnce();
    expect(executeQuarantineDeletion).not.toHaveBeenCalled();
    expect(updateQuarantinePolicy).not.toHaveBeenCalled();
    await wrapper.get("button.secondary").trigger("click");
    await flushPromises();
    expect(
      wrapper.get("button.primary").attributes("disabled"),
    ).toBeUndefined();
  });

  it("discards a deletion challenge returned after leaving the page", async () => {
    const wrapper = mountCachedPage();
    await flushPromises();
    await wrapper.get('input[type="checkbox"]').setValue(true);
    let finish!: () => void;
    const prepared = await prepareQuarantineDeletion(["one"]);
    vi.mocked(prepareQuarantineDeletion).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = () => resolve(prepared);
        }),
    );
    await wrapper.get("button.danger-outline").trigger("click");
    await wrapper.get('[aria-label="切换页面"]').trigger("click");
    finish();
    await flushPromises();
    await wrapper.get('[aria-label="切换页面"]').trigger("click");
    await flushPromises();
    expect(wrapper.text()).not.toContain("此操作无法恢复");
    expect(executeQuarantineDeletion).not.toHaveBeenCalled();
  });
});
