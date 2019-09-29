import { CustomElement } from "./Custom";

@CustomElement('x-main-header')
export class Header extends HTMLElement {
    private _pageHeader: string = "";
    private shadow: ShadowRoot;

    private constructor() {
        super()

        // Root of the shadow DOM
        let shadow = this.attachShadow({mode: 'open'});
        let style = document.createElement("style");
        style.textContent = HeaderStyle;
        shadow.innerHTML = HeaderTemplate;

        shadow.appendChild(style);
        this.shadow = shadow;
    }

    get pageHeader(): string {
        return this._pageHeader;
    }

    set pageHeader(value: string) {
        this.setAttribute('page-header', value);
    }

    static get observedAttributes(): string[] {
        return ['page-header']
    }

    attributeChangedCallback(name: string, old: string|null, value: string|null): void {
        switch (name) {
            case 'page-header':
                let pageHeader = this.shadow.getElementById('pageHeader');
                if (pageHeader != null) pageHeader.innerText = value || "";
                this._pageHeader = value || "";
                break;
        }
    }

    connectedCallback() {
    }

    disconnectedCallback() {
    }

    static createElement(): Header {
        return document.createElement('x-main-header') as Header;
    }
}

const HeaderStyle: string = `
.main-header {
    display: flex;
    flex-direction: row;
    align-items: center;

    background: rgb(255,255,255);
    background: linear-gradient(0deg, rgba(255,255,255,1) 0%, rgba(240,240,240,1) 10%, rgba(240,240,240,1) 90%, rgba(255,255,255,1) 100%);
}

.logo {
    margin: 4px;
}

.filler {
    flex: 1 1 auto;
}

.appName {
    font-weight: bold;
}
`;

const HeaderTemplate: string = `
<header class="main-header">
    <div class="logo"><img src="/web/favicon.png" alt="Logo" height="32"></img></div>
    <div><span class="appName">Photo Archive v0.1</span></div>
    <div class="filler"></div>
    <div><span id="pageHeader"></span></div>
    <div class="filler"></div>
    <div>TODO Menu</div>
</header>
`