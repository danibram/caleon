import { forwardRef } from 'react'
import type { AskOptions } from '../context/AppContext.tsx'

export type AskUiState = AskOptions & { inputValue: string }

export const AskDialog = forwardRef<
  HTMLDialogElement,
  {
    askUi: AskUiState | null
    setAskUi: (v: AskUiState | null) => void
    onResolve: (value: string | boolean | null) => void
  }
>(function AskDialog({ askUi, setAskUi, onResolve }, dlgRef) {
  const closeAsk = (ok: boolean) => {
    if (!askUi) return
    if (!ok) onResolve(null)
    else onResolve(askUi.input === false ? true : askUi.inputValue)
    setAskUi(null)
  }

  return (
    <dialog
      ref={dlgRef}
      id="dlg"
      onClose={(e) => {
        if (askUi) closeAsk(e.currentTarget.returnValue === 'ok')
      }}
    >
      <form
        method="dialog"
        onSubmit={(e) => {
          e.preventDefault()
          const dlg = e.currentTarget.closest('dialog')
          if (!dlg) return
          dlg.returnValue = 'ok'
          dlg.close()
        }}
      >
        {askUi ? (
          <>
            <div style={{ fontWeight: 600 }}>{askUi.title}</div>
            {askUi.text ? <div className="muted small">{askUi.text}</div> : null}
            {askUi.input === false ? null : (
              <>
                <label htmlFor="dlgInput">{askUi.label}</label>
                <input
                  id="dlgInput"
                  type={askUi.type || 'text'}
                  value={askUi.inputValue}
                  maxLength={askUi.max || undefined}
                  onChange={(e) =>
                    setAskUi({ ...askUi, inputValue: e.target.value })
                  }
                />
                {askUi.presets?.length ? (
                  <div className="presets">
                    {askUi.presets.map(([label, val], i) => (
                      <button
                        key={i}
                        type="button"
                        className={`chip${askUi.inputValue === val ? ' active' : ''}`}
                        onClick={() => setAskUi({ ...askUi, inputValue: val })}
                      >
                        {label}
                      </button>
                    ))}
                  </div>
                ) : null}
              </>
            )}
            <div className="actions" style={{ justifyContent: 'flex-end' }}>
              <button
                type="button"
                className="btn"
                onClick={() => {
                  const dlg =
                    typeof dlgRef === 'function' ? null : dlgRef?.current
                  dlg?.close()
                }}
              >
                Cancelar
              </button>
              <button type="submit" className="btn primary" value="ok">
                {askUi.ok || 'Aceptar'}
              </button>
            </div>
          </>
        ) : null}
      </form>
    </dialog>
  )
})
