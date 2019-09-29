export function CustomElement(name: string)  {
    return function(target: any) {
        customElements.define(name, target);
    }
}