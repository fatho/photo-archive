import { CustomElement } from "./Custom";
import { Position } from "../util/Position";
import { Lru } from "../util/Cache";

@CustomElement('virtual-grid')
export class VirtualGrid extends HTMLElement {
    private elementCache: Lru<number, HTMLElement>;

    /// How many columns/rows fit into the  visible space
    private visibleGridWidth: number = 0;
    private visibleGridHeight: number = 0;
    /// How many grid rows are there in total
    private gridRowCount: number = 0;
    /// How many virtualized elements should be created at most
    private maximumCachedElementCount: number = 0;

    private viewport: HTMLElement;
    private viewportSizer: HTMLElement;

    private renderer: VirtualGridRenderer;

    // cached so that we don't have to parse the attribute string every time
    private _cellWidth: number = 0;
    private _cellHeight: number = 0;
    private _virtualElementCount: number = 0;

    private onWindowResizeHandler: (this: Window, event: Event) => void;

    private constructor() {
        super()

        this.elementCache = new Lru();


        // Root of the shadow DOM
        let shadow = this.attachShadow({mode: 'open'});

        let viewport = document.createElement('div');
        viewport.setAttribute('class', 'viewport');
        Position.absolute(viewport).fill();

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
            this.virtualizeCells(false);
        };

        this.viewport = viewport;
        this.viewportSizer = viewportSizer;

        // use the default renderer initially
        this.renderer = new DebugGridRenderer();

        let that = this;
        this.onWindowResizeHandler = function(this: Window, event: Event) {
            that.computeBounds();
        };
    }


    private computeBounds() {
        let rect = this.getBoundingClientRect();

        this.visibleGridWidth = Math.max(1, Math.floor(rect.width / this.cellWidth));
        this.visibleGridHeight = Math.floor(rect.height / this.cellHeight) + 2;
        this.gridRowCount = Math.ceil(this.virtualElementCount / this.visibleGridWidth);

        let pixelHeight = this.gridRowCount * this.cellHeight;
        this.viewportSizer.style.height = pixelHeight + "px";

        // cache four "pages" worth of elements
        this.maximumCachedElementCount = Math.min(this.virtualElementCount, this.visibleGridWidth * (this.visibleGridHeight * 6));

        this.virtualizeCells(true);
    }

    private virtualizeCells(layoutChanged: boolean) {
        let rect = this.getBoundingClientRect();
        let scrollTop = this.viewport.scrollTop;
        let scrollBottom = scrollTop + rect.height;

        let cellWidth = this.cellWidth;
        let cellHeight = this.cellHeight;

        // create elements for the current page as well as the one before and after
        let gridTop = Math.max(0, Math.floor(scrollTop / cellHeight) - this.visibleGridHeight);
        let gridBottom = Math.min(this.gridRowCount, Math.ceil(scrollBottom / cellHeight) + this.visibleGridHeight);

        let horizontalPadding = Math.max(0, (rect.width - cellWidth * this.visibleGridWidth) / (this.visibleGridWidth + 1));

        let itemCount = this.virtualElementCount;

        for(let y = gridTop; y < gridBottom; y++) {
            for(let x = 0; x < this.visibleGridWidth; x++) {
                let itemIndex = y * this.visibleGridWidth + x;
                if (itemIndex < itemCount) {
                    let cell = this.elementCache.get(itemIndex);
                    if (cell == undefined) {
                        cell = this.getCell();
                        this.renderer.assignVirtualizedElement(cell, itemIndex);
                        this.elementCache.insert(itemIndex, cell);

                        if ( ! layoutChanged ) {
                            // If the layout didn't change, just place the new element where it belongs.
                            // Otherwise, we will recompute all element positions afterwards.
                            let top = y * cellHeight;
                            let left = horizontalPadding + x * (horizontalPadding + cellWidth);

                            Position.absolute(cell).top(top + 'px').left(left + 'px').width(cellWidth + 'px').height(cellHeight + 'px');
                        }
                    }
                }
            }
        }

        if (layoutChanged) {
            // If the layout changed, we must set all element positions again, otherwise they might suddenly overlap
            this.elementCache.forEach((itemIndex, cell) => {
                let x = itemIndex % this.visibleGridWidth;
                let y = Math.floor(itemIndex / this.visibleGridWidth);

                let top = y * cellHeight;
                let left = horizontalPadding + x * (horizontalPadding + cellWidth);

                Position.absolute(cell).top(top + 'px').left(left + 'px').width(cellWidth + 'px').height(cellHeight + 'px');
            });
        }
    }

    private getCell(): HTMLElement {
        if (this.elementCache.size > this.maximumCachedElementCount) {
            // reuse old one
            return this.elementCache.evict(1)[0].value;
        } else {
            let elem = this.renderer.createVirtualizedElement();
            this.viewport.appendChild(elem);
            return elem;
        }
    }

    invalidateAllItems(redraw: boolean): void {
        this.elementCache.forEach((key, value) => {
            this.viewport.removeChild(value);
            this.renderer.abandonVirtualizedElement(value);
        });
        this.elementCache.clear();

        if ( redraw ) {
            this.virtualizeCells(false);
        }
    }

    get itemRenderer(): VirtualGridRenderer {
        return this.renderer;
    }

    set itemRenderer(renderer: VirtualGridRenderer) {
        if (renderer == null || renderer == undefined) {
            throw new Error('Renderer must not be null/undefined');
        }

        if (renderer != this.renderer) {
            this.invalidateAllItems(false);

            this.renderer = renderer;
            // make new elements
            this.virtualizeCells(false)
        }
    }

    get cellWidth(): number {
        return this._cellWidth;
    }

    set cellWidth(value: number) {
        this.setAttribute('cell-width', value.toString());
    }

    get cellHeight(): number {
        return this._cellHeight;
    }

    set cellHeight(value: number) {
        this.setAttribute('cell-height', value.toString());
    }

    get virtualElementCount(): number {
        return this._virtualElementCount;
    }

    set virtualElementCount(value: number) {
        this.setAttribute('virtual-element-count', value.toString());
    }

    static get observedAttributes(): string[] {
        return ['virtual-element-count', 'cell-width', 'cell-height']
    }

    attributeChangedCallback(name: string, old: string|null, value: string|null): void {
        switch(name) {
            case 'cell-width':
                this._cellWidth = parseInt(value as string) || 0;
                break;
            case 'cell-height':
                this._cellHeight = parseInt(value as string) || 0;
                break;
            case 'virtual-element-count':
                this._virtualElementCount = parseInt(value as string) || 0;
                break;
        }
        this.computeBounds();
    }

    connectedCallback() {
        this.computeBounds();
        window.addEventListener('resize', this.onWindowResizeHandler);
    }

    disconnectedCallback() {
        window.removeEventListener('resize', this.onWindowResizeHandler);
    }

    static createElement(): VirtualGrid {
        return document.createElement('virtual-grid') as VirtualGrid;
    }
}

export interface VirtualGridRenderer {
    /// Create the HTMLElement that is used for rendering items.
    createVirtualizedElement(): HTMLElement;

    /// Called when the given element (that was previously created with `createVirtualizedElement`)
    /// is used for presenting the item with index `index`.
    assignVirtualizedElement(element: HTMLElement, index: number): void;

    /// Called when the virtualized element is removed from the DOM because it is not needed anymore.
    abandonVirtualizedElement(element: HTMLElement): void;
}

class DebugGridRenderer implements VirtualGridRenderer {
    /// By default creates an empty DIV. Can be overriden derived classes to generate a more complex DOM.
    createVirtualizedElement(): HTMLElement {
        return document.createElement("div");
    }

    /// Called when the given element (that was previously created with `createVirtualizedElement`)
    /// is used for presenting the item with index `index`.
    assignVirtualizedElement(element: HTMLElement, index: number) {
        // DEBUG:
        element.innerText = "Hello " + index;
    }

    /// Called when the virtualized element is removed from the DOM because it is not needed anymore.
    abandonVirtualizedElement(element: HTMLElement) { }
}
