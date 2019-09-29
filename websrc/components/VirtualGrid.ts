import { CustomElement } from "./Custom";
import { Position } from "../util/Position";
import { Lru } from "../util/Cache";

@CustomElement('virtual-grid')
export class VirtualGrid extends HTMLElement {
    private elementCache: Lru<number, HTMLElement>;

    private visibleGridWidth: number = 0;
    private visibleGridHeight: number = 0;
    private gridRowCount: number = 0;

    private viewport: HTMLElement;
    private viewportSizer: HTMLElement;

    private constructor() {
        super()

        this.elementCache = new Lru();


        // Root of the shadow DOM
        let shadow = this.attachShadow({mode: 'open'});

        let viewport = document.createElement('div');
        viewport.setAttribute('class', 'viewport');
        Position.absolute(viewport).fill();
        viewport.style.backgroundColor = "lightblue";

        let viewportSizer = document.createElement('div');
        Position.absolute(viewportSizer).left('0px').top('0px').width('1px').height('0px');
        viewportSizer.style.visibility = "hidden";

        let style = document.createElement("style");
        style.textContent = `
            .viewport {
                overflow-x: hidden;
                overflow-y: scroll;
            }`;

        shadow.appendChild(style);
        shadow.appendChild(viewport);
        viewport.appendChild(viewportSizer);
        viewport.onscroll = () => {
            this.virtualizeCells();
        };

        this.viewport = viewport;
        this.viewportSizer = viewportSizer;
    }


    private computeBounds() {
        let rect = this.getBoundingClientRect();

        this.visibleGridWidth = Math.floor(rect.width / this.cellWidth);
        this.visibleGridHeight = Math.floor(rect.height / this.cellHeight) + 2;
        this.gridRowCount = Math.ceil(this.virtualElementCount / this.visibleGridWidth);

        let pixelHeight = this.gridRowCount * this.cellHeight;
        this.viewportSizer.style.height = pixelHeight + "px";

        console.log("Visible grid: %d * %d", this.visibleGridWidth, this.visibleGridHeight);
        console.log("Rows: %d", this.gridRowCount);

        this.virtualizeCells();
    }

    private virtualizeCells() {
        let rect = this.getBoundingClientRect();
        let scrollTop = this.viewport.scrollTop;
        let scrollBottom = scrollTop + rect.height;

        let cellWidth = this.cellWidth;
        let cellHeight = this.cellHeight;

        let gridTop = Math.floor(scrollTop / cellHeight);
        let gridBottom = Math.ceil(scrollBottom / cellHeight);

        let itemCount = this.virtualElementCount;

        for(let y = gridTop; y < gridBottom; y++) {
            for(let x = 0; x < this.visibleGridWidth; x++) {
                let itemIndex = y * this.visibleGridWidth + x;
                if (itemIndex < itemCount) {
                    let cell = this.elementCache.get(itemIndex);
                    if (cell == undefined) {
                        cell = this.getCell();
                        this.elementCache.insert(itemIndex, cell);

                        //cell.innerText = "Hello " + itemIndex;
                        cell.setAttribute('src', '/photos/' + (itemIndex + 1) + '/thumbnail');

                        let top = y * cellHeight;
                        let left = x * cellWidth;

                        Position.absolute(cell).top(top + 'px').left(left + 'px').width(cellWidth + 'px').height(cellHeight + 'px');
                    }
                }
            }
        }
    }

    private getCell(): HTMLElement {
        if (this.elementCache.size > Math.max(1, this.visibleGridWidth * this.visibleGridHeight) * 4) {
            // reuse old one
            return this.elementCache.evict(1)[0].value;
        } else {
            console.log("new element");
            let elem = document.createElement("img");
            //elem.style.backgroundColor = 'gray';
            this.viewport.appendChild(elem);
            return elem;
        }
    }


    get cellWidth(): number {
        return parseInt(this.getAttribute('cell-width') as string);
    }

    set cellWidth(value: number) {
        this.setAttribute('cell-width', value.toString());
    }

    get cellHeight(): number {
        return parseInt(this.getAttribute('cell-height') as string);
    }

    set cellHeight(value: number) {
        this.setAttribute('cell-height', value.toString());
    }

    get virtualElementCount(): number {
        return parseInt(this.getAttribute('virtual-element-count') as string) || 0;
    }

    set virtualElementCount(value: number) {
        this.setAttribute('virtual-element-count', value.toString());
    }

    static get observedAttributes(): string[] {
        return ['virtual-element-count', 'cell-width', 'cell-height']
    }

    attributeChangedCallback(name: string, old: string|null, value: string|null): void {
        console.log("VirtualGrid attribute changed %s: %s -> %s", name, old, value);
        this.computeBounds();
    }

    connectedCallback() {
        console.log("VirtualGrid connected");
        this.computeBounds();
    }

    disconnectedCallback() {
        console.log("VirtualGrid disconnected");
    }

    static createElement(): VirtualGrid {
        return document.createElement('virtual-grid') as VirtualGrid;
    }
}