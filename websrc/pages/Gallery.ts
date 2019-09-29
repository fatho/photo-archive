import { Page } from "./Page";
import { Request } from "../util/AjaxRequest";
import { VirtualGrid, VirtualGridRenderer } from "../components/VirtualGrid";
import { Position } from "../util/Position";

export class GalleryPage implements Page {
    imageGrid: VirtualGrid;
    photos: Photo[];

    constructor() {
        this.imageGrid = VirtualGrid.createElement();
        Position.absolute(this.imageGrid).fill();
        this.imageGrid.cellWidth = 320;
        this.imageGrid.cellHeight = 240;
        this.imageGrid.itemRenderer = new GalleryItemRenderer(this);
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
        root.appendChild(this.imageGrid);
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