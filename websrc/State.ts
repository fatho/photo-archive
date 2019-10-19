import { Request } from "./util/AjaxRequest";

export class AppState {
    private _photosChanged: Set<StateChangedListener> = new Set();
    private _photos: Photo[] = new Array();

    requestPhotos(): void {
        Request.get('/photos')
            .onSuccess(r => this.receivePhotos(r.json()))
            .onFailure(r => this.failedPhotos(r.text()))
            .send();
    }

    public get photos() : Photo[] {
        return this._photos;
    }

    private failedPhotos(message: string): void {
        console.log(message);
    }

    private receivePhotos(newPhotos: Photo[]): void {
        newPhotos.sort(createdOrderNullsLast);
        let oldPhotos = this._photos;
        this._photos = newPhotos;

        this._photosChanged.forEach((handler) => handler.photosChanged(this, oldPhotos, newPhotos));
    }

    addPhotosChangedListener(listener: StateChangedListener): void {
        this._photosChanged.add(listener);
    }

    removePhotosChangedListener(listener: StateChangedListener): void {
        this._photosChanged.delete(listener);
    }
}

export interface StateChangedListener {
    photosChanged(state: AppState, oldPhotos: Photo[], newPhotos: Photo[]): void;
}

export type Photo = {
    id: number,
    relative_path: string,
    created: string | null,
};

/// Most recent photos come first, photos without a created date come last.
function createdOrderNullsLast(a: Photo, b: Photo): number {
    if ( a.created == null && b.created == null) {
        return a.id - b.id;
    } else if (a.created == null) {
        return 1;
    } else if (b.created == null) {
        return -1;
    } else {
        return -a.created.localeCompare(b.created);
    }
}