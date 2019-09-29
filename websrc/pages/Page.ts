export interface Page {
    render(root: HTMLElement): void;

    enter(): void;

    leave(): void;
}