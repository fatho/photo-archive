import { Page } from "./Page";
import { Request } from "../util/AjaxRequest";
import { VirtualGrid, VirtualGridRenderer, ViewportChangedEventData } from "../components/VirtualGrid";
import { Position } from "../util/Position";
import { Header } from "../components/Header";

export class GalleryPage implements Page {
    flexContainer: HTMLElement;
    imageGrid: VirtualGrid;
    header: Header;
    photos: Photo[];

    constructor() {
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
            let created = that.photos[e.detail.firstVirtualIndex].created;
            let startTimestamp = "unknown date";
            if (created) {
                let timestamp = new Date(created);
                startTimestamp = timestamp.toLocaleString('de-DE')
            }
            that.header.pageHeader = `${startTimestamp} (${e.detail.firstVirtualIndex} of ${that.photos.length})`;
        } as EventListener);

        this.photos = new Array();
    }

    enter(): void {
        Request.get('/photos')
            .onSuccess(r => this.receivePhotos(r.json()))
            .onFailure(r => this.failedPhotos(r.text()))
            .send();
    }

    leave(): void {}

    failedPhotos(message: string) {
        console.log(message);
    }

    receivePhotos(photos: Photo[]) {
        console.log(photos.length);
        photos.sort((a, b) => {
            if ( a.created == null && b.created == null) {
                return a.id - b.id;
            } else if (a.created == null) {
                return 1;
            } else if (b.created == null) {
                return -1;
            } else {
                return -a.created.localeCompare(b.created);
            }
        });
        this.photos = photos;
        this.imageGrid.virtualElementCount = this.photos.length;
        this.imageGrid.invalidateAllItems(true);
    }

    render(root: HTMLElement): void {
        root.appendChild(this.flexContainer);
    }
}

type Photo = {
    id: number,
    relative_path: string,
    created: string,
};

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
        return cell;
    }

    assignVirtualizedElement(element: HTMLElement, index: number): void {
        // Look up the photo, and assign the url
        if ( index < this.page.photos.length ) {
            let photo = this.page.photos[index];
            element.style.backgroundImage = `url("/photos/${photo.id}/thumbnail")`
        }
    }

    abandonVirtualizedElement(element: HTMLElement): void { }
}