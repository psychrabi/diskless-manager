import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import WindowsServicingModal from "./WindowsServicingModal";
const mocks = vi.hoisted(() => ({ capabilities: vi.fn(), prepare: vi.fn(), success: vi.fn(), error: vi.fn(), confirm: vi.fn() }));
vi.mock("@/api/modules/images", () => ({ listMasters: async () => [{id:"win",name:"diskless/image-disk/windows11",os_type:"windows",status:"ready",snapshots:[{name:"installed"}]}] }));
vi.mock("@/api/modules/networkDrivers", () => ({listNetworkDrivers: async () => [{id:"netkvm",name:"VirtIO Ethernet"}]}));
vi.mock("@/api/modules/windowsServicing", () => ({getRemoteWindowsServicingCapabilities: mocks.capabilities, prepareWindowsImageManaged: mocks.prepare, prepareWindowsImageRemote: vi.fn(), prepareWindowsImageRemoteCatalog: vi.fn()}));
vi.mock("@/store/useToastStore", () => ({useToastStore: () => ({success:mocks.success,error:mocks.error})}));
vi.mock("@/contexts/confirmDialog", () => ({useConfirm: () => mocks.confirm}));
beforeEach(() => {
  vi.clearAllMocks();
  mocks.capabilities.mockResolvedValue({available:true,elevated:true,platform:"windows",protocol_version:2,iscsi_volume_servicing:true});
  mocks.confirm.mockResolvedValue(true);
  mocks.prepare.mockResolvedValue({preparation:{committed:true,drivers_added:1},image:{name:"windows11-serviced"},snapshot:{name:"serviced-test"},staged_package_count:1,uploaded_bytes:100,staging_cleanup_succeeded:true});
});
async function fillForm() {
  fireEvent.change(screen.getByLabelText("Windows host"),{target:{value:"192.168.123.10"}});
  fireEvent.change(screen.getByLabelText("SSH username"),{target:{value:"diskless"}});
  fireEvent.click(screen.getByRole("button",{name:"Check Worker"}));
  await screen.findByText(/Ready · windows/);
  await screen.findByText("diskless/image-disk/windows11");
  fireEvent.change(screen.getByLabelText("Source Windows image"),{target:{value:"win"}});
  fireEvent.change(screen.getByLabelText("Source snapshot"),{target:{value:"installed"}});
  fireEvent.change(screen.getByLabelText("New image name"),{target:{value:"windows11-serviced"}});
  fireEvent.change(screen.getByLabelText("Server iSCSI address"),{target:{value:"192.168.123.1"}});
  fireEvent.click(await screen.findByRole("checkbox",{name:/VirtIO Ethernet/}));
}
it("services a selected managed snapshot and refreshes images after saving",async () => {
  const onPrepared=vi.fn();
  render(<WindowsServicingModal isOpen onPrepared={onPrepared}/>);
  await fillForm();
  fireEvent.click(screen.getByRole("button",{name:"Prepare and Commit"}));
  await waitFor(() => expect(mocks.prepare).toHaveBeenCalledWith(expect.objectContaining({host:"192.168.123.10",image_id:"win",source_snapshot:"installed",new_name:"windows11-serviced",portal:"192.168.123.1",driver_ids:["netkvm"],commit:true})));
  expect(await screen.findByText(/Saved windows11-serviced@serviced-test/)).toBeInTheDocument();
  expect(onPrepared).toHaveBeenCalledOnce();
});
it("requires a helper that supports managed volumes",async () => {
  mocks.capabilities.mockResolvedValue({available:true,elevated:true,platform:"windows",protocol_version:2});
  render(<WindowsServicingModal isOpen/>);
  await fillForm();
  expect(screen.getByRole("button",{name:"Prepare and Commit"})).toBeDisabled();
  expect(screen.getByRole("alert")).toHaveTextContent("Update the Windows helper");
  expect(mocks.prepare).not.toHaveBeenCalled();
});
