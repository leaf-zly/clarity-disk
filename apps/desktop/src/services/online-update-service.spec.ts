import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { check } from "@tauri-apps/plugin-updater";
import {
  checkOnlineUpdate,
  updateErrorMessage,
} from "@/services/online-update-service";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), isTauri: vi.fn() }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn() }));
const resource = {
  version: "0.2.0",
  body: "new release",
  download: vi.fn(),
  install: vi.fn(),
  close: vi.fn(),
};
describe("online update service", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(check).mockResolvedValue(
      resource as unknown as NonNullable<Awaited<ReturnType<typeof check>>>,
    );
    vi.mocked(invoke).mockResolvedValue(undefined);
    resource.download.mockResolvedValue(undefined);
    resource.install.mockResolvedValue(undefined);
    resource.close.mockResolvedValue(undefined);
  });
  it("does not invoke native updates in the browser", async () => {
    vi.mocked(isTauri).mockReturnValue(false);
    await expect(checkOnlineUpdate()).rejects.toThrow("仅在已安装");
    expect(check).not.toHaveBeenCalled();
  });
  it("checks with a bounded timeout and no downgrade override", async () => {
    vi.mocked(check).mockResolvedValue(null);
    expect(await checkOnlineUpdate()).toBeNull();
    expect(check).toHaveBeenCalledWith({ timeout: 20_000 });
  });
  it("requires verified download before taking the native install reservation", async () => {
    const update = (await checkOnlineUpdate())!;
    await expect(update.install()).rejects.toThrow("尚未完成签名校验");
    expect(invoke).not.toHaveBeenCalled();
    const progress = vi.fn();
    await update.download(progress);
    expect(resource.download).toHaveBeenCalledWith(progress, {
      timeout: 300_000,
    });
    await update.install();
    expect(invoke).toHaveBeenNthCalledWith(1, "reserve_update_installation");
    expect(resource.install).toHaveBeenCalledOnce();
    expect(invoke).toHaveBeenNthCalledWith(2, "release_update_installation");
  });
  it("never installs after a failed signature verification", async () => {
    resource.download.mockRejectedValueOnce(new Error("signature mismatch"));
    const update = (await checkOnlineUpdate())!;
    await expect(update.download(vi.fn())).rejects.toThrow(
      "signature mismatch",
    );
    await expect(update.install()).rejects.toThrow("尚未完成签名校验");
    expect(resource.install).not.toHaveBeenCalled();
  });
  it("does not launch installer while a native operation holds the gate", async () => {
    const update = (await checkOnlineUpdate())!;
    await update.download(vi.fn());
    vi.mocked(invoke).mockRejectedValueOnce(new Error("仍有操作正在执行"));
    await expect(update.install()).rejects.toThrow("正在执行");
    expect(resource.install).not.toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledOnce();
  });
  it("releases the native gate after installer launch failure", async () => {
    const update = (await checkOnlineUpdate())!;
    await update.download(vi.fn());
    resource.install.mockRejectedValueOnce(new Error("launch failed"));
    await expect(update.install()).rejects.toThrow("launch failed");
    expect(invoke).toHaveBeenLastCalledWith("release_update_installation");
    await update.close();
    expect(resource.close).toHaveBeenCalledOnce();
    await expect(update.install()).rejects.toThrow("尚未完成签名校验");
  });
  it("distinguishes verification, connectivity and active operation failures", () => {
    expect(updateErrorMessage("signature invalid")).toContain("已禁止安装");
    expect(updateErrorMessage(new Error("request timed out"))).toContain(
      "检查网络",
    );
    expect(updateErrorMessage("仍有操作正在执行")).toBe("仍有操作正在执行");
  });
});
