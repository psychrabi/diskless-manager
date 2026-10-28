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

**Restore-point apply**:
Creating a new master image from an existing snapshot without changing the source image, its newer snapshots, or client assignments.
_Avoid_: rollback, when the source history is preserved.

**Draft image**:
A hidden image volume created by an image-factory session to receive an installation. It is not assignable or listed until a verified normal boot publishes it.
_Avoid_: master, while the image is still a draft.

**Verification candidate**:
A disposable clone of a draft snapshot that is booted as a normal client to prove the image works.
_Avoid_: test image, when the exact session-owned clone matters.

**Verification target**:
The session-owned iSCSI target that exposes only the verification candidate, using a per-attempt IQN.
_Avoid_: install target, which exposes the draft and installer media.

**Published image**:
A draft image that a verification boot has proven; its status becomes `ready` and it becomes assignable.
_Avoid_: final image, when publication is the specific verified step.

**Manual boot**:
An interactive installer or tooling boot on a scratch draft. It never publishes an image and discards its scratch disk when the session closes.
_Avoid_: image factory, when no image is built.
