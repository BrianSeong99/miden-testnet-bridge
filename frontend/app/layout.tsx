import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "Miden Bridge Lab",
  description: "Operational lab UI for the Miden bridge mock service.",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
