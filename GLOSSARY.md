# Diskless Manager

Terms for managing shared Windows images and the clients that boot from them.

## Language

**Windows boot network driver**:
A Windows network-adapter driver needed during startup to keep a diskless client connected to its iSCSI boot disk.
_Avoid_: NIC driver, when referring specifically to the driver’s boot-time role.

**Boot NIC**:
The client network adapter Windows uses to reach its iSCSI boot disk.
_Avoid_: primary NIC, unless it is also the adapter carrying the iSCSI boot connection.

**Windows image revision**:
A published version of a Windows boot image with a defined set of drivers. A client boots from the revision assigned to it.
_Avoid_: image, when the exact published driver state matters.
