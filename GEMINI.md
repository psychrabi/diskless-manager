# Project Overview

This is a Tauri application for managing diskless PXE/iSCSI boot environments. The frontend is built with React and Vite, and the backend is built with Rust.

The application provides a web-based interface for managing ZFS, iSCSI, DHCP, and TFTP. It allows users to:

- Create and manage master images
- Manage snapshots for quick rollback
- Add, edit, and remove diskless clients
- Monitor client status
- Configure and manage network services

# Building and Running

## Development

To run the application in development mode, use the following command:

```bash
bun tauri dev
```

This will start the Vite development server for the frontend and the Tauri development server for the backend.

## Production

To build the application for production, use the following command:

```bash
bun tauri build
```

This will create a production-ready executable in the `src-tauri/target/release` directory.

## Testing

To run the test suite, use the following command:

```bash
bun test
```

# Development Conventions

## Frontend

- The frontend code is located in the `src` directory.
- Components are located in the `src/components` directory.
- The main entry point for the frontend is `src/main.jsx`.
- The frontend uses React, Vite, and Tailwind CSS.

## Backend

- The backend code is located in the `src-tauri` directory.
- The main entry point for the backend is `src-tauri/src/main.rs`.
- The backend is a Tauri application written in Rust.
- The backend exposes a number of commands to the frontend using the `tauri::command` macro.
- The main application logic is in `src-tauri/src/lib.rs`.
