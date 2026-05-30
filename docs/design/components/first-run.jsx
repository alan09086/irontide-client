// IronTide — first-run setup wizard. Multi-step, shown on first launch
// (and reachable via Help → Setup Wizard). Emerald-dark, KDE-native.
(() => {
  const { Icon, IT, Chrome } = window;

  const STEPS = ['Welcome', 'Downloads', 'Connection', 'Privacy', 'Done'];

  function FirstRunWizard({ onClose }) {
    const [step, setStep] = React.useState(0);
    const next = () => setStep(s => Math.min(STEPS.length - 1, s + 1));
    const back = () => setStep(s => Math.max(0, s - 1));
    const last = step === STEPS.length - 1;

    const Field = ({ label, hint, children }) => (
      <div style={{ marginBottom: 14 }}>
        <div style={{ fontSize: 12, color: 'var(--fg-1)', marginBottom: 5 }}>{label}</div>
        {children}
        {hint ? <div style={{ fontSize: 11, color: 'var(--fg-3)', marginTop: 5 }}>{hint}</div> : null}
      </div>
    );

    return (
      <div style={{
        position: 'absolute', inset: 0, background: 'rgba(0,0,0,0.55)',
        display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 65,
      }} onClick={onClose}>
        <div onClick={e => e.stopPropagation()} style={{
          width: 640, maxWidth: '94vw', height: 520, maxHeight: '92vh',
          background: 'var(--bg-0)', border: '1px solid var(--border-1)',
          borderRadius: 'var(--r-lg)', boxShadow: 'var(--shadow-lg)',
          display: 'flex', overflow: 'hidden',
        }}>
          {/* Step rail */}
          <div style={{ width: 180, flexShrink: 0, background: 'var(--bg-1)', borderRight: '1px solid var(--border-1)', padding: '20px 0', display: 'flex', flexDirection: 'column' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '0 18px 18px' }}>
              <Chrome.BoltMark size={16} />
              <span style={{ fontSize: 13, fontWeight: 600 }}>IronTide</span>
            </div>
            {STEPS.map((s, i) => (
              <div key={s} style={{
                display: 'flex', alignItems: 'center', gap: 10, padding: '8px 18px',
                color: i === step ? 'var(--fg-0)' : i < step ? 'var(--fg-2)' : 'var(--fg-3)',
                fontSize: 12.5,
                borderLeft: i === step ? '2px solid var(--accent)' : '2px solid transparent',
              }}>
                <span style={{
                  width: 18, height: 18, borderRadius: 9, flexShrink: 0, fontSize: 11,
                  display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
                  background: i < step ? 'var(--accent)' : i === step ? 'var(--accent-bg-soft)' : 'var(--bg-3)',
                  color: i < step ? 'var(--accent-fg)' : i === step ? 'var(--accent)' : 'var(--fg-3)',
                  border: i === step ? '1px solid var(--accent)' : '1px solid transparent',
                }}>{i < step ? Icon.check({ size: 11 }) : i + 1}</span>
                {s}
              </div>
            ))}
          </div>

          {/* Step content */}
          <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minWidth: 0 }}>
            <div style={{ flex: 1, overflowY: 'auto', padding: 24 }}>
              {step === 0 && (
                <div>
                  <div style={{ display: 'flex', justifyContent: 'center', margin: '12px 0 18px' }}>
                    <Chrome.BoltMark size={48} />
                  </div>
                  <div style={{ fontSize: 20, fontWeight: 700, textAlign: 'center', letterSpacing: '-0.02em' }}>Welcome to IronTide</div>
                  <div style={{ fontSize: 13, color: 'var(--fg-2)', textAlign: 'center', marginTop: 10, lineHeight: 1.6, maxWidth: 380, marginInline: 'auto' }}>
                    A fast, focused BitTorrent client. Let's set a few defaults — you can change everything later in Preferences.
                  </div>
                </div>
              )}
              {step === 1 && (
                <div>
                  <div style={{ fontSize: 15, fontWeight: 600, marginBottom: 16 }}>Where should downloads go?</div>
                  <Field label="Default save path">
                    <div style={{ display: 'flex', gap: 6 }}>
                      <IT.TextInput mono value="/home/alan/Downloads" width="100%" />
                      <IT.Btn variant="solid">Browse…</IT.Btn>
                    </div>
                  </Field>
                  <Field label="Use a separate folder for incomplete downloads" hint="Keeps in-progress files out of your finished library until they're done.">
                    <IT.Toggle on label="Enabled" />
                  </Field>
                  <Field label="When adding a torrent">
                    <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                      <IT.Toggle on label="Show the add-torrent dialog" />
                      <IT.Toggle on={false} label="Start in paused state" />
                    </div>
                  </Field>
                </div>
              )}
              {step === 2 && (
                <div>
                  <div style={{ fontSize: 15, fontWeight: 600, marginBottom: 16 }}>Connection</div>
                  <Field label="Incoming connection port" hint="A fixed port helps peers reach you. Leave random if you're unsure.">
                    <div style={{ display: 'flex', gap: 10, alignItems: 'center' }}>
                      <IT.TextInput mono value="6881" width={120} />
                      <IT.Toggle on={false} label="Randomize on each start" />
                    </div>
                  </Field>
                  <Field label="Port forwarding">
                    <IT.Toggle on label="Map port with UPnP / NAT-PMP" />
                  </Field>
                  <div style={{ marginTop: 8, padding: 12, background: 'var(--bg-1)', border: '1px solid var(--border-1)', borderRadius: 'var(--r-md)', display: 'flex', alignItems: 'center', gap: 8 }}>
                    <IT.StatusDot tone="seeding" />
                    <span style={{ fontSize: 12.5, color: 'var(--fg-1)' }}>Port <span className="mono">6881</span> is reachable — incoming connections OK.</span>
                  </div>
                </div>
              )}
              {step === 3 && (
                <div>
                  <div style={{ fontSize: 15, fontWeight: 600, marginBottom: 16 }}>Privacy & peer discovery</div>
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
                    <IT.Toggle on label="DHT (distributed hash table)" />
                    <IT.Toggle on label="Peer Exchange (PeX)" />
                    <IT.Toggle on label="Local Service Discovery (LSD)" />
                  </div>
                  <Field label="Encryption" hint="Require encryption on private trackers; prefer it elsewhere.">
                    <IT.Select value="Prefer encryption" options={['Prefer encryption', 'Require encryption', 'Disable encryption']} width={240} />
                  </Field>
                  <IT.Toggle on={false} label="Anonymous mode (hide client fingerprint)" />
                </div>
              )}
              {step === 4 && (
                <div>
                  <div style={{ display: 'flex', justifyContent: 'center', margin: '12px 0 18px', color: 'var(--accent)' }}>
                    <span style={{ width: 56, height: 56, borderRadius: 28, background: 'var(--accent-bg-soft)', border: '1px solid var(--accent)', display: 'inline-flex', alignItems: 'center', justifyContent: 'center' }}>{Icon.check({ size: 28 })}</span>
                  </div>
                  <div style={{ fontSize: 18, fontWeight: 700, textAlign: 'center' }}>You're all set</div>
                  <div style={{ fontSize: 13, color: 'var(--fg-2)', textAlign: 'center', marginTop: 10, lineHeight: 1.6, maxWidth: 360, marginInline: 'auto' }}>
                    IronTide is ready. Drop a <span className="mono">.torrent</span>, paste a magnet, or press <IT.Kbd>Ctrl</IT.Kbd><IT.Kbd>K</IT.Kbd> to find anything.
                  </div>
                </div>
              )}
            </div>

            {/* Footer */}
            <div style={{ height: 56, flexShrink: 0, borderTop: '1px solid var(--border-1)', background: 'var(--bg-1)', display: 'flex', alignItems: 'center', padding: '0 18px', gap: 8 }}>
              <span style={{ flex: 1, fontSize: 11, color: 'var(--fg-3)' }}>Step {step + 1} of {STEPS.length}</span>
              {step > 0 && !last ? <IT.Btn variant="ghost" onClick={back}>Back</IT.Btn> : null}
              {!last ? <IT.Btn variant="primary" onClick={next} style={{ whiteSpace: 'nowrap' }}>{step === 0 ? 'Get started' : 'Next'}</IT.Btn>
                     : <IT.Btn variant="primary" onClick={onClose} icon={Icon.bolt({ size: 13 })} style={{ whiteSpace: 'nowrap' }}>Start IronTide</IT.Btn>}
            </div>
          </div>
        </div>
      </div>
    );
  }

  window.FirstRunWizard = FirstRunWizard;
})();
