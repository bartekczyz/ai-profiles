type Props = {
  color: string
}

export function ManagedSidebarSwatch({ color }: Props) {
  return (
    <span
      aria-hidden
      className="inline-block h-2.5 w-2.5 rounded-full shadow-[inset_0_0_0_1px_rgba(0,0,0,0.08),0_1px_1px_rgba(0,0,0,0.06)]"
      style={{ background: color }}
    />
  )
}
