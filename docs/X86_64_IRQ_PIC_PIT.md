# x86-64 Legacy IRQ, PIC, and PIT

- **Status:** PIC, returning IRQ path, and PIT IRQ0 smoke verified in QEMU
- **Scope:** 8259 PIC vectors 32–47, returning IRQ entry, handler registry, 8254 PIT channel 0
- **Runtime policy:** IRQ lines remain masked by default; no production-wide STI is enabled

## Interrupt path

- CPU exceptions 0–31 retain the existing fatal exception path.
- Legacy PIC IRQs 0–15 map to IDT vectors 32–47.
- IRQ stubs normalize the stack with a dummy error code and vector, save all general-purpose registers, and call the returning Rust dispatcher.
- The dispatcher validates the vector, checks spurious IRQ7/IRQ15 before invoking a callback, then sends EOI after a real handler returns.
- The assembly path restores the saved registers and returns with `iretq`.

## Handler registration and enabling

- `irq::register_handler(irq, handler)` registers a write-once callback in an atomic table.
- `irq::unmask(irq)` requires a registered callback and is unsafe: the caller must run at ring 0 with CPU interrupts disabled and exclusive PIC ownership.
- Slave IRQs also unmask the master cascade line (IRQ2).
- Registering a callback does not unmask its line. PIC initialization leaves both masks at `0xff`.
- Unknown/unregistered real IRQs fail-stop rather than returning through an unhandled interrupt.

## PIT channel 0

- `pit::initialize_periodic(hz)` programs channel 0 in periodic square-wave mode.
- Input clock is 1,193,182 Hz; the driver rejects zero frequencies and divisors outside the 16-bit counter range.
- PIT programming does not register a handler, unmask IRQ0, or enable CPU interrupts.

## Tick accounting

`timer::initialize_pit(hz)` installs the kernel-owned IRQ0 callback, programs PIT channel 0, records the integer actual frequency, and unmasks IRQ0. It does not execute `STI`; the caller remains responsible for enabling CPU interrupts only after all dependent kernel state is ready. `timer::ticks()`, `timer::frequency_hz()`, and `timer::uptime_millis()` expose the monotonic tick count and derived time after initialization. This is a single-CPU early timebase foundation, not yet a scheduler clock, wall clock, or SMP time source.

## QEMU validation

The `irq-smoke-pit` build-only feature registers an IRQ0 callback, programs the PIT at 100 Hz, verifies the PIC mask and IF state, unmasks IRQ0, and waits for one tick. The handler increments an atomic counter; the smoke path then disables interrupts and masks IRQ0 again.

The CI smoke test runs on `-machine pc -cpu qemu64,-apic` to isolate the legacy 8259 route. It registers IRQ0, programs channel 0 at 100 Hz, unmasks IRQ0, observes the handler's atomic tick counter after a `HLT` wake-up, then disables interrupts and masks IRQ0 again. The `IRQ_PIT_SMOKE_OK` serial marker is required by CI. This validates the legacy PIC/PIT IRQ0 delivery, handler dispatch, register restoration and `iretq`; it does **not** validate IOAPIC/LAPIC routing on q35, SMP delivery, scheduler ticks, or production-wide interrupt enabling.

## Safety constraints

- Do not add global `sti` to the normal bootstrap path.
- Do not unmask an IRQ before its handler is registered and its device is ready.
- Do not enable PIT-driven scheduling until interrupt routing, timekeeping, preemption, and SMP synchronization have dedicated contracts and tests.
