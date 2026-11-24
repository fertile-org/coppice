import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './styles/tokens.css'
import './index.css'
import App from './App.tsx'
import { TextSizeProvider } from './features/theme/TextSizeProvider'
import { ThemeProvider } from './features/theme/ThemeProvider'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <ThemeProvider>
      <TextSizeProvider>
        <App />
      </TextSizeProvider>
    </ThemeProvider>
  </StrictMode>,
)
