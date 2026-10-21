<script lang="ts" module>
  // 24×24 stroke icons drawn for NANOFreeDisk (round caps, 1.8 stroke).
  const PATHS: Record<string, string> = {
    home: "M3 11 12 4l9 7M5 10v10h5v-6h4v6h5V10",
    analyze: "M12 3v9h9M12 3a9 9 0 1 0 9 9M12 3a9 9 0 0 1 9 9",
    clean: "M12 3l1.8 4.6L18 9l-4.2 1.4L12 15l-1.8-4.6L6 9l4.2-1.4zM19 15l.8 2 2 .8-2 .8-.8 2-.8-2-2-.8 2-.8zM5 15l.6 1.4 1.4.6-1.4.6L5 19l-.6-1.4L3 17l1.4-.6z",
    devcache: "M8 7l-5 5 5 5M16 7l5 5-5 5M14 4l-4 16",
    apps: "M21 8 12 3 3 8v8l9 5 9-5zM3 8l9 5 9-5M12 13v8",
    duplicates: "M8 8h11v12H8zM5 16V4h11",
    monitor: "M3 12h4l3-8 4 16 3-8h4",
    health: "M12 20s-8-4.6-8-10.2A4.6 4.6 0 0 1 12 7a4.6 4.6 0 0 1 8 2.8C20 15.4 12 20 12 20zM7 12h3l1.5-2 2 4 1.5-2h2",
    settings: "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z",
    folder: "M3 6.5A1.5 1.5 0 0 1 4.5 5H9l2 2.5h8.5A1.5 1.5 0 0 1 21 9v9.5a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 18.5z",
    file: "M14 3H6.5A1.5 1.5 0 0 0 5 4.5v15A1.5 1.5 0 0 0 6.5 21h11a1.5 1.5 0 0 0 1.5-1.5V8zM14 3v5h5",
    link: "M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1",
    up: "M12 19V5M5 12l7-7 7 7",
    refresh: "M20 11a8 8 0 0 0-14.6-4.5L4 8M4 4v4h4M4 13a8 8 0 0 0 14.6 4.5L20 16m0 4v-4h-4",
    export: "M12 3v12M7 10l5 5 5-5M5 21h14",
    search: "M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14zM20 20l-4-4",
    x: "M6 6l12 12M18 6 6 18",
    trash: "M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3",
    undo: "M9 14 4 9l5-5M4 9h10.5a5.5 5.5 0 0 1 0 11H11",
    reveal: "M3 6.5A1.5 1.5 0 0 1 4.5 5H9l2 2.5h8.5A1.5 1.5 0 0 1 21 9v9.5a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 18.5zM12 11v5M9.5 13.5 12 11l2.5 2.5",
    open: "M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5",
    zoom: "M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14zM20 20l-4-4M11 8v6M8 11h6",
    copy: "M9 9h11v11H9zM5 15V4h11",
    drive: "M3 13h18v5a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1zM3 13l3-8h12l3 8M7 16h.01M11 16h2",
    usb: "M12 3v14M8 7l4-4 4 4M7 12v2l5 3 5-3v-3M12 21a2 2 0 1 0 0-4 2 2 0 0 0 0 4z",
    network: "M4 5h16v8H4zM12 13v4M5 20h14M12 17v3",
    disc: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 14a2 2 0 1 0 0-4 2 2 0 0 0 0 4z",
    alert: "M12 3 2 20h20zM12 10v4M12 17h.01",
    check: "M5 12l5 5 9-10",
    chevron: "M9 6l6 6-6 6",
    cloud: "M7 18h10a4 4 0 0 0 .5-8A6 6 0 0 0 6 9.5 4.3 4.3 0 0 0 7 18z",
    lock: "M6 11h12v9H6zM8 11V8a4 4 0 0 1 8 0v3",
    shield: "M12 3l8 3v6c0 4.5-3.4 8-8 9-4.6-1-8-4.5-8-9V6z",
    sun: "M12 17a5 5 0 1 0 0-10 5 5 0 0 0 0 10zM12 1v2M12 21v2M4.2 4.2l1.4 1.4M18.4 18.4l1.4 1.4M1 12h2M21 12h2M4.2 19.8l1.4-1.4M18.4 5.6l1.4-1.4",
    sunburst: "M12 12m-3 0a3 3 0 1 0 6 0 3 3 0 1 0-6 0M12 3a9 9 0 1 1-9 9M12 6.5a5.5 5.5 0 1 1-5.5 5.5",
    treemap: "M3 4h18v16H3zM12 4v16M12 11h9M3 14h9M16.5 11v9",
    filter: "M3 5h18l-7 8v6l-4 2v-8z",
    cpu: "M7 7h10v10H7zM10 10h4v4h-4zM9 3v4M15 3v4M9 17v4M15 17v4M3 9h4M3 15h4M17 9h4M17 15h4",
    board: "M4 4h16v16H4zM8 8h4v4H8zM16 7v3M16 14v3h-2M7 16h4",
    memory: "M3 8h18v8H3zM7 11v2M11 11v2M15 11v2M6 16v3M18 16v3",
    gpu: "M3 7h18v10H3zM6 17v3M11 12m-2 0a2 2 0 1 0 4 0 2 2 0 1 0-4 0M16 10h2M16 14h2",
    screen: "M3 5h18v11H3zM8 20h8M12 16v4",
    battery: "M3 8h15v8H3zM18 11h3v2h-3M6 11v2M9 11v2",
    thermo: "M14 14.8V5a2 2 0 0 0-4 0v9.8a4 4 0 1 0 4 0zM12 9v7",
    tasks: "M9 6h11M9 12h11M9 18h11M4.5 6h.01M4.5 12h.01M4.5 18h.01",
    power: "M13 2 4 14h7l-1 8 9-12h-7z",
    fan: "M12 14a2 2 0 1 0 0-4 2 2 0 0 0 0 4zM12 10c0-4 1-7 4-7s2 5-2 7M14 12c4 0 7 1 7 4s-5 2-7-2M12 14c0 4-1 7-4 7s-2-5 2-7M10 12c-4 0-7-1-7-4s5-2 7 2",
    clock: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 7v5l3 2",
    gauge: "M4 18a8 8 0 1 1 16 0M12 18l4-6",
    agent: "M8 8h8a3 3 0 0 1 3 3v5a3 3 0 0 1-3 3H8a3 3 0 0 1-3-3v-5a3 3 0 0 1 3-3zM12 8V5M12 4h.01M9.5 13v1M14.5 13v1M3 13v2M21 13v2",
    data: "M5 6c0-1.7 3.1-3 7-3s7 1.3 7 3-3.1 3-7 3-7-1.3-7-3zM5 6v12c0 1.7 3.1 3 7 3s7-1.3 7-3V6M5 12c0 1.7 3.1 3 7 3s7-1.3 7-3",
  };
</script>

<script lang="ts">
  let { name, size = 18, title = "" }: { name: string; size?: number; title?: string } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.8"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden={title ? undefined : "true"}
  role={title ? "img" : undefined}
  aria-label={title || undefined}
  style="flex: none"
>
  <path d={PATHS[name] ?? PATHS.file} />
</svg>
