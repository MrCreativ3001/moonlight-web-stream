import path from "node:path";
import { fileURLToPath } from "node:url";
import HtmlWebpackPlugin from 'html-webpack-plugin';
import CopyPlugin from "copy-webpack-plugin";

// In Node.js versions prior to native support for import.meta.dirname,
// derive __dirname from import.meta.url.
// (Node 20.11+ supports import.meta.dirname and import.meta.filename.)
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

export default {
    // TODO: change based on env
    mode: "development",
    entry: {
        index: "./web/index.ts",
        stream: "./web/stream.ts"
    },
    module: {
        rules: [
            {
                test: /\.tsx?$/,
                use: "ts-loader",
                exclude: /node_modules/,
            },
            {
                test: /\.css$/i,
                use: [
                    {
                        loader: 'style-loader',
                        options: {
                            injectType: 'lazyStyleTag'
                        }
                    },
                    'css-loader'
                ]
            },
            {
                test: /\.(png|svg|jpg|jpeg|gif)$/i,
                type: 'asset/resource',
            },
            {
                test: /\.(wasm)$/i,
                type: 'asset/resource',
            },
        ],
    },
    plugins: [
        new HtmlWebpackPlugin({
            filename: 'index.html',
            template: './web/index.html',
            chunks: ['index'],
            scriptLoading: 'blocking'
        }),
        new HtmlWebpackPlugin({
            filename: 'stream.html',
            template: './web/stream.html',
            chunks: ['stream'],
            scriptLoading: 'blocking'
        }),
        new CopyPlugin({
            patterns: [
                {
                    from: "./web/manifest.json",
                    to: "manifest.json"
                },
            ],
        }),
    ],
    resolve: {
        // TODO: migrate all files to ts
        extensions: [".ts", ".js"],
        extensionAlias: {
            ".js": [".ts", ".js"]
        }
    },
    output: {
        filename: "[name].js",
        path: path.resolve(__dirname, "dist"),
        clean: true
    },
    externals: {
        "./config.js": "window.__RUNTIME_CONFIG__"
    }
};