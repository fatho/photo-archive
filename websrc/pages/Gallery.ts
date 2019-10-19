import { Page } from "./Page";
import { VirtualGrid, VirtualGridRenderer, ViewportChangedEventData } from "../components/VirtualGrid";
import { Position } from "../util/Position";
import { Header } from "../components/Header";
import { AppState, StateChangedListener } from "../State";
import { HashRouter } from "../routing/HashRouter";

export class GalleryPage implements Page, StateChangedListener {
    private flexContainer: HTMLElement;
    private imageGrid: VirtualGrid;
    private header: Header;
    router: HashRouter;
    state: AppState;

    constructor(state: AppState, router: HashRouter) {
        this.state = state;
        this.router = router;

        // setup flexbox layout
        this.flexContainer = document.createElement('div');
        // make the flex container fill all of its parent
        Position.absolute(this.flexContainer).fill();
        // lay out the children in a single column
        this.flexContainer.style.display = "flex";
        this.flexContainer.style.flexDirection = "column";

        this.header = Header.createElement();
        this.flexContainer.appendChild(this.header);
        // make the header only take as much space as needed
        this.header.style.flex = "0 1 auto";

        this.imageGrid = VirtualGrid.createElement();
        // grow and shrink as needed, taking all the remaining space
        this.imageGrid.style.flex = "1 1 auto";
        this.imageGrid.style.position = 'relative';
        this.flexContainer.appendChild(this.imageGrid);
        this.imageGrid.cellWidth = 320;
        this.imageGrid.cellHeight = 240;
        this.imageGrid.itemRenderer = new GalleryItemRenderer(this);
        let that = this;
        this.imageGrid.addEventListener('viewportchanged', function(this: HTMLElement, e: CustomEvent<ViewportChangedEventData>) {
            if (e.detail.firstVirtualIndex >= that.state.photos.length) {
                return;
            }

            let created = that.state.photos[e.detail.firstVirtualIndex].created;
            let startTimestamp = "unknown date";
            if (created) {
                let timestamp = new Date(created);
                startTimestamp = timestamp.toLocaleString('de-DE')
            }
            that.header.pageHeader = `${startTimestamp} (${e.detail.firstVirtualIndex + 1} of ${that.state.photos.length})`;
        } as EventListener);
    }

    private refreshView() {
        console.log("view refreshed: %d", this.state.photos.length);
        this.imageGrid.virtualElementCount = this.state.photos.length;
        this.imageGrid.invalidateAllItems(true);
    }

    photosChanged(): void {
        console.log("GalleryPage: photos changed");
        this.refreshView();
    }

    attach(root: HTMLElement): void {
        root.appendChild(this.flexContainer);
        this.state.addPhotosChangedListener(this)
        this.refreshView();
    }

    detach(root: HTMLElement): void {
        this.state.removePhotosChangedListener(this)
        root.removeChild(this.flexContainer);
    }
}

class GalleryItemRenderer implements VirtualGridRenderer {
    page: GalleryPage;

    constructor(page: GalleryPage) {
        this.page = page;
    }

    createVirtualizedElement(): HTMLElement {
        // We will render gallery items as background-image of the div.
        let cell = document.createElement('div');
        cell.style.backgroundSize = 'fill';
        cell.style.backgroundPosition = 'center';
        cell.style.backgroundRepeat = 'no-repeat';
        cell.style.backgroundSize = 'contain';
        return cell;
    }

    assignVirtualizedElement(element: HTMLElement, index: number): void {
        let page = this.page
        // Look up the photo, and assign the url
        if ( index < page.state.photos.length ) {
            let photo = page.state.photos[index];
            element.style.backgroundImage = `url("/photos/${photo.id}/thumbnail")`;
            element.onclick = () => {
                page.router.navigate(['slideshow', index.toString()]);
            };
        }
    }

    abandonVirtualizedElement(element: HTMLElement): void { }
}