# Diskless Manager - Project Context

## Overview
Diskless Manager is a web-based toolkit for managing diskless PXE/iSCSI boot environments using ZFS, iSCSI, DHCP, and TFTP. It's built as a Tauri desktop application using React for the frontend and Rust for the backend, providing a comprehensive solution for managing diskless computing environments.

## Architecture
- **Frontend**: React 19.2 with Tailwind CSS, DaisyUI, and React Router
- **Backend**: Rust with Tauri framework
- **Architecture**: Desktop application with web-based UI
- **State Management**: Zustand for frontend, custom AppState for backend
- **Styling**: Tailwind CSS with DaisyUI components

## Core Features
- ZFS Management (create/manage master images, snapshots, clones)
- Network Boot Configuration (iSCSI targets, DHCP/PXE, TFTP)
- Client Management (add/edit/remove, status monitoring, Wake-on-LAN)
- Service Management (DHCP, TFTP, Apache, Samba, iSCSI)
- System Monitoring and Logging

## Project Structure
```
diskless-manager/
├── src/                    # Frontend React source code
│   ├── components/        # React components
│   ├── contexts/          # React context providers
│   ├── hooks/            # Custom React hooks
│   ├── lib/              # Utility functions
│   ├── router/           # Routing configuration
│   ├── store/            # State management
│   ├── utils/            # Utility functions
│   └── index.css         # Global styles
├── src-tauri/            # Rust backend source code
│   ├── src/              # Rust source files
│   │   ├── commands/     # Tauri command handlers
│   │   ├── core/         # Core business logic
│   │   ├── services/     # System service integrations
│   │   ├── state/        # Application state management
│   │   ├── types/        # Type definitions
│   │   └── utils/        # Utility functions
│   ├── Cargo.toml        # Rust dependencies
│   └── tauri.conf.json   # Tauri configuration
├── package.json          # Node.js dependencies and scripts
├── vite.config.js        # Vite build configuration
└── README.md             # Project documentation
```

## Key Technologies
- **Frontend**: React 19.2, React Router 7.11.0, Tailwind CSS 4.1.18, DaisyUI
- **Backend**: Rust, Tauri 2.9.6, Tokio async runtime
- **State Management**: Zustand, React Context
- **UI Components**: Lucide React icons, React Hook Form, Zod for validation
- **Testing**: Vitest, React Testing Library
- **Styling**: Tailwind CSS with DaisyUI components

## Building and Running

### Prerequisites
- Bun (JavaScript runtime)
- Rust (for Tauri backend)
- System dependencies (ZFS, iSCSI, DHCP, TFTP, etc.)

### Development Setup
```bash
# Install dependencies
bun install

# Start development server
bun tauri dev

# Or run frontend separately
bun dev
```

### Production Build
```bash
# Build the application
bun tauri build
```

### Available Scripts
- `bun dev` - Start Vite development server
- `bun tauri dev` - Start Tauri development with hot reload
- `bun tauri build` - Build production application
- `bun build` - Build frontend only
- `bun test` - Run tests with Vitest
- `bun lint` - Lint code with ESLint
- `bun lint:fix` - Fix linting issues

## Security Configuration
The application requires a JWT secret for authentication. Set the `JWT_SECRET` environment variable with a secure random value generated using:
```bash
openssl rand -base64 32
```

## Key Frontend Components
- Authentication system with login and protected routes
- Admin layout with sidebar navigation
- Client management interface
- Image management system
- Service management dashboard
- Settings and configuration panels
- Error boundary and loading components
- Notification and confirmation dialog contexts

## Key Backend Modules
- **Auth**: Authentication and JWT token management
- **Client**: Client management (add, edit, delete, control)
- **Config**: Configuration management
- **ZFS**: ZFS operations (pools, datasets, snapshots)
- **iSCSI**: iSCSI target management
- **DHCP**: DHCP server configuration
- **Services**: System service control and monitoring
- **Logs**: Application logging
- **License**: License management

## Testing
- Unit tests using Vitest
- React component testing with React Testing Library
- Test setup in `src/test/setup.js`

## Development Conventions
- React components use JSX/TSX with functional components and hooks
- Tauri commands are defined in Rust and exposed to frontend
- State management follows React best practices with Zustand and Context
- Error handling is implemented consistently across the application
- Asynchronous operations use async/await pattern
- Type safety enforced with Zod schema validation
- ESLint and Tailwind CSS for code style consistency

## Environment Requirements
The application integrates with system-level services:
- ZFS filesystem management
- iSCSI target configuration
- DHCP server configuration
- TFTP server management
- Samba file sharing
- Wake-on-LAN functionality