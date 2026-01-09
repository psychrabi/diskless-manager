import { lazy } from "react";

// Properly implement lazy loading with dynamic imports
const Activity = lazy(() =>
  import("./Activity.jsx").then((module) => ({ default: module.Activity })),
);
const Card = lazy(() =>
  import("./Card.jsx").then((module) => ({ default: module.Card })),
);
const Button = lazy(() =>
  import("./Button.jsx").then((module) => ({ default: module.Button })),
);
const Modal = lazy(() =>
  import("./Modal.jsx").then((module) => ({ default: module.Modal })),
);
const Input = lazy(() =>
  import("./Input.jsx").then((module) => ({ default: module.Input })),
);
const Select = lazy(() =>
  import("./Select.jsx").then((module) => ({ default: module.Select })),
);
const ContextMenu = lazy(() =>
  import("./ContextMenu.jsx").then((module) => ({
    default: module.ContextMenu,
  })),
);
const Loading = lazy(() =>
  import("./Loading.jsx").then((module) => ({ default: module.Loading })),
);
const Notification = lazy(() =>
  import("./Notification.jsx").then((module) => ({
    default: module.Notification,
  })),
);
const Error = lazy(() =>
  import("./Error.jsx").then((module) => ({ default: module.Error })),
);
const StatusBadge = lazy(() =>
  import("./StatusBadge.jsx").then((module) => ({ default: module.StatusBadge })),
);
const LoadingSkeleton = lazy(() =>
  import("./LoadingSkeleton.jsx").then((module) => ({ default: module.LoadingSkeleton })),
);
const TableSkeleton = lazy(() =>
  import("./LoadingSkeleton.jsx").then((module) => ({ default: module.TableSkeleton })),
);
const CardSkeleton = lazy(() =>
  import("./LoadingSkeleton.jsx").then((module) => ({ default: module.CardSkeleton })),
);

export * from "./Table";

export {
  Activity,
  Button,
  Card,
  CardSkeleton,
  ContextMenu,
  Error,
  Input,
  Loading,
  LoadingSkeleton,
  Modal,
  Notification,
  Select,
  StatusBadge,
  TableSkeleton,
};
