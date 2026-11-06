/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  // Dark-only UI: no `dark:` variants anywhere, so the media/class strategy is irrelevant.
  theme: {
    extend: {
      keyframes: {
        indeterminate: {
          "0%": { transform: "translateX(-100%)" },
          "100%": { transform: "translateX(300%)" },
        },
      },
      animation: {
        indeterminate: "indeterminate 1.2s ease-in-out infinite",
      },
    },
  },
  plugins: [],
};
