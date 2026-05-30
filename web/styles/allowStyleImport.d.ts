interface LazyStyleModule {
    use(): void;
    unuse(): void;
}

declare module "*.css" {

    const styles: LazyStyleModule;
    export default styles;
}