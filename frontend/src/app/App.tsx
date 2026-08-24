import { lazy } from 'react';
import { Route, Routes } from 'react-router-dom';
import { AppShell } from '../components/layout/AppShell';

const HomePage = lazy(async () => ({ default: (await import('../pages/HomePage')).HomePage }));
const IdeaComposerPage = lazy(async () => ({ default: (await import('../pages/IdeaComposerPage')).IdeaComposerPage }));
const ResearchListPage = lazy(async () => ({ default: (await import('../pages/ResearchListPage')).ResearchListPage }));
const ResearchDetailPage = lazy(async () => ({ default: (await import('../pages/ResearchDetailPage')).ResearchDetailPage }));
const AlphaLibraryPage = lazy(async () => ({ default: (await import('../pages/AlphaLibraryPage')).AlphaLibraryPage }));
const AlphaDetailPage = lazy(async () => ({ default: (await import('../pages/AlphaDetailPage')).AlphaDetailPage }));
const PortfolioLabPage = lazy(async () => ({ default: (await import('../pages/PortfolioLabPage')).PortfolioLabPage }));
const PortfolioCandidatePage = lazy(async () => ({ default: (await import('../pages/PortfolioCandidatePage')).PortfolioCandidatePage }));
const ApprovalInboxPage = lazy(async () => ({ default: (await import('../pages/ApprovalInboxPage')).ApprovalInboxPage }));
const HandoffFeedbackPage = lazy(async () => ({ default: (await import('../pages/HandoffFeedbackPage')).HandoffFeedbackPage }));
const AdministrationPage = lazy(async () => ({ default: (await import('../pages/AdministrationPage')).AdministrationPage }));
const NotFoundPage = lazy(async () => ({ default: (await import('../pages/NotFoundPage')).NotFoundPage }));

export function App() {
  return (
    <Routes>
      <Route element={<AppShell />}>
        <Route index element={<HomePage />} />
        <Route path="ideas" element={<IdeaComposerPage />} />
        <Route path="research" element={<ResearchListPage />} />
        <Route path="research/:id" element={<ResearchDetailPage />} />
        <Route path="alphas" element={<AlphaLibraryPage />} />
        <Route path="alphas/:id" element={<AlphaDetailPage />} />
        <Route path="portfolio" element={<PortfolioLabPage />} />
        <Route path="portfolio/candidates/:id" element={<PortfolioCandidatePage />} />
        <Route path="approvals" element={<ApprovalInboxPage />} />
        <Route path="handoffs" element={<HandoffFeedbackPage />} />
        <Route path="admin" element={<AdministrationPage />} />
        <Route path="*" element={<NotFoundPage />} />
      </Route>
    </Routes>
  );
}
