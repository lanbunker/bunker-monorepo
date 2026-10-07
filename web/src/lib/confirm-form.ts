/**
 * "yes" takes no click for this long after the question shows, so a double
 * click on the action cannot also confirm it.
 */
export const ARM_DELAY_MS = 400

/**
 * Asks before every `form[data-confirm]` submits, inside the page. A native
 * `window.confirm` is not reliable: a browser that blocks dialogs answers
 * "cancel" without showing one, and the action never runs. Without script the
 * form submits as it is.
 */
export const enableConfirmations = (doc: Document): void => {
    const template = doc.querySelector<HTMLTemplateElement>("template#confirm-prompt")
    if (!template) return

    let closeOpen: Close | undefined
    let asked = 0
    for (const form of doc.querySelectorAll<HTMLFormElement>("form[data-confirm]")) {
        form.addEventListener("submit", event => {
            if (form.dataset.confirmed === "yes") return
            event.preventDefault()
            if (form.querySelector("[data-confirm-prompt]")) return
            // While a "yes" is on its way, no other action starts.
            if (closeOpen && !closeOpen()) return
            asked += 1
            closeOpen = ask(form, template, `confirm-text-${asked}`, () => {
                closeOpen = undefined
            })
        })
    }
    doc.addEventListener("keydown", event => {
        if (event.key === "Escape") closeOpen?.()
    })
    // Back after a "yes" can bring the page out of the cache with the question
    // still open and its buttons off, so the form starts over.
    window.addEventListener("pageshow", event => {
        if (event.persisted) closeOpen?.({ sent: true })
    })
}

/**
 * Takes the question back and returns the focus, and answers whether it did.
 * Once "yes" has sent the form, only `{ sent: true }` does, so neither Escape
 * nor another question can bring the action back for a second send while the
 * first is on its way.
 */
type Close = (options?: { sent: boolean }) => boolean

/** Shows the question in place of the form's buttons, with the focus on "no". */
const ask = (
    form: HTMLFormElement,
    template: HTMLTemplateElement,
    textId: string,
    onClose: () => void,
): Close => {
    const prompt = template.content.firstElementChild?.cloneNode(true)
    if (!(prompt instanceof HTMLElement)) return () => true
    const text = prompt.querySelector("[data-confirm-text]")
    const yes = prompt.querySelector("[data-confirm-yes] button")
    const no = prompt.querySelector("[data-confirm-no] button")
    if (
        !text ||
        !(yes instanceof HTMLButtonElement) ||
        !(no instanceof HTMLButtonElement)
    ) {
        return () => true
    }

    // Both answers carry the question as their description, so a screen reader
    // that lands on "no" says what the answer is for.
    text.id = textId
    for (const answer of [yes, no]) answer.setAttribute("aria-describedby", textId)
    text.textContent = form.dataset.confirm ?? "are you sure?"
    const actions = [...form.querySelectorAll<HTMLButtonElement>("button")]
    for (const action of actions) action.hidden = true
    yes.disabled = true
    form.append(prompt)
    const timer = window.setTimeout(() => {
        yes.disabled = false
    }, ARM_DELAY_MS)

    let sending = false
    const close: Close = options => {
        if (sending && !options?.sent) return false
        window.clearTimeout(timer)
        prompt.remove()
        for (const action of actions) action.hidden = false
        actions[0]?.focus()
        onClose()
        return true
    }
    no.addEventListener("click", () => close())
    yes.addEventListener("click", () => {
        // A field the browser refuses stops the send, so the question gives
        // the form back instead of waiting on buttons that are off.
        if (!form.reportValidity()) {
            close()
            return
        }
        sending = true
        yes.disabled = true
        no.disabled = true
        // The submit event runs during requestSubmit, so the flag only lets
        // this one submit through.
        form.dataset.confirmed = "yes"
        form.requestSubmit()
        delete form.dataset.confirmed
    })
    no.focus()

    return close
}
