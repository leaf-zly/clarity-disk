import {
  enableAutoUnmount,
  flushPromises,
  mount,
  type VueWrapper,
} from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import OnlineUpdateCard from "@/components/OnlineUpdateCard.vue";
import {
  checkOnlineUpdate,
  type OnlineUpdate,
} from "@/services/online-update-service";

vi.mock("@/services/online-update-service", async (importOriginal) => ({
  ...(await importOriginal<
    typeof import("@/services/online-update-service")
  >()),
  checkOnlineUpdate: vi.fn(),
}));
enableAutoUnmount(afterEach);
let resource: OnlineUpdate;

/** Finds an action by the text users see, without accessing component methods. */
function button(wrapper: VueWrapper, text: string) {
  const result = wrapper.findAll("button").find((item) => item.text() === text);
  if (!result) throw new Error(`Missing button: ${text}`);
  return result;
}
describe("OnlineUpdateCard", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    resource = {
      version: "0.2.0",
      notes: "<script>untrusted</script>",
      download: vi.fn().mockResolvedValue(undefined),
      install: vi.fn().mockResolvedValue(undefined),
      close: vi.fn().mockResolvedValue(undefined),
    };
    vi.mocked(checkOnlineUpdate).mockResolvedValue(resource);
  });
  it("never checks automatically or when update consent is disabled", async () => {
    const wrapper = mount(OnlineUpdateCard, { props: { enabled: false } });
    await flushPromises();
    await button(wrapper, "检查更新").trigger("click");
    expect(checkOnlineUpdate).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain("测试版通道");
    expect(wrapper.text()).toContain("Windows 发布者签名");
  });
  it("renders release notes as text and requires separate download and install confirmation", async () => {
    const wrapper = mount(OnlineUpdateCard, { props: { enabled: true } });
    await button(wrapper, "检查更新").trigger("click");
    await flushPromises();
    expect(wrapper.text()).toContain("发现新版本 0.2.0");
    expect(wrapper.text()).toContain("<script>untrusted</script>");
    expect(wrapper.find("script").exists()).toBe(false);
    expect(resource.download).not.toHaveBeenCalled();
    await button(wrapper, "下载更新").trigger("click");
    await flushPromises();
    expect(resource.install).not.toHaveBeenCalled();
    expect(button(wrapper, "安装并重启").attributes("disabled")).toBeDefined();
    await wrapper.get('input[type="checkbox"]').setValue(true);
    await button(wrapper, "安装并重启").trigger("click");
    await flushPromises();
    expect(resource.install).toHaveBeenCalledOnce();
  });
  it("does not equate download Finished with successful signature verification", async () => {
    let reject!: (error: Error) => void;
    resource.download = vi.fn((onEvent) => {
      onEvent({ event: "Started", data: { contentLength: 100 } });
      onEvent({ event: "Progress", data: { chunkLength: 100 } });
      onEvent({ event: "Finished" });
      return new Promise<void>((_, fail) => {
        reject = fail;
      });
    });
    const wrapper = mount(OnlineUpdateCard, { props: { enabled: true } });
    await button(wrapper, "检查更新").trigger("click");
    await flushPromises();
    await button(wrapper, "下载更新").trigger("click");
    expect(wrapper.text()).toContain("正在验证签名");
    expect(wrapper.text()).not.toContain("安装并重启");
    reject(new Error("signature mismatch"));
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain("已禁止安装");
    expect(resource.install).not.toHaveBeenCalled();
    expect(resource.close).toHaveBeenCalledOnce();
  });
  it("allows retry after a failed check without displaying up-to-date", async () => {
    vi.mocked(checkOnlineUpdate).mockRejectedValueOnce(
      new Error("network timeout"),
    );
    const wrapper = mount(OnlineUpdateCard, { props: { enabled: true } });
    await button(wrapper, "检查更新").trigger("click");
    await flushPromises();
    expect(wrapper.text()).not.toContain("已是此通道的最新版本");
    vi.mocked(checkOnlineUpdate).mockResolvedValueOnce(null);
    await button(wrapper, "检查更新").trigger("click");
    await flushPromises();
    expect(wrapper.text()).toContain("已是此通道的最新版本");
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
  });
  it("deduplicates pending checks and releases late resources on unmount", async () => {
    let finish!: (value: OnlineUpdate) => void;
    vi.mocked(checkOnlineUpdate).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const wrapper = mount(OnlineUpdateCard, { props: { enabled: true } });
    await button(wrapper, "检查更新").trigger("click");
    await flushPromises();
    await button(wrapper, "正在检查…").trigger("click");
    expect(checkOnlineUpdate).toHaveBeenCalledOnce();
    wrapper.unmount();
    finish(resource);
    await flushPromises();
    expect(resource.close).toHaveBeenCalledOnce();
  });
});
