import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { Card } from './Card';
import { Settings } from 'lucide-react';

describe('Card', () => {
    it('renders with title', () => {
        render(<Card title="Test Card">Content</Card>);
        expect(screen.getByText('Test Card')).toBeInTheDocument();
    });

    it('renders without title when not provided', () => {
        render(<Card>Content</Card>);
        expect(screen.queryByRole('heading')).not.toBeInTheDocument();
    });

    it('renders children', () => {
        render(<Card title="Card"><p>Child content</p></Card>);
        expect(screen.getByText('Child content')).toBeInTheDocument();
    });

    it('renders with icon', () => {
        const { container } = render(<Card title="Settings" icon={Settings}>Content</Card>);
        // Icon should be rendered as SVG
        const svg = container.querySelector('svg');
        expect(svg).toBeInTheDocument();
    });

    it('renders without icon when not provided', () => {
        const { container } = render(<Card title="Card">Content</Card>);
        // Check that no SVG icon is rendered before the title
        const heading = screen.getByRole('heading');
        const svg = heading.parentElement?.parentElement?.querySelector('svg');
        expect(svg).not.toBeInTheDocument();
    });

    it('renders actions', () => {
        render(
            <Card title="Card" actions={<button>Action</button>}>
                Content
            </Card>
        );
        expect(screen.getByRole('button', { name: 'Action' })).toBeInTheDocument();
    });

    it('applies custom className', () => {
        const { container } = render(<Card className="custom-class">Content</Card>);
        expect(container.querySelector('.card')).toHaveClass('custom-class');
    });

    it('applies custom titleClassName', () => {
        render(<Card title="Title" titleClassName="custom-title">Content</Card>);
        expect(screen.getByRole('heading')).toHaveClass('custom-title');
    });

    it('applies custom bodyClass', () => {
        const { container } = render(<Card bodyClass="custom-body">Content</Card>);
        const cardBodies = container.querySelectorAll('.card-body');
        const mainBody = Array.from(cardBodies).find(el => el.textContent === 'Content');
        expect(mainBody).toHaveClass('custom-body');
    });

    it('applies custom headerClass', () => {
        const { container } = render(<Card title="Title" headerClass="custom-header">Content</Card>);
        const cardBodies = container.querySelectorAll('.card-body');
        const headerBody = Array.from(cardBodies).find(el => el.querySelector('h3'));
        expect(headerBody).toHaveClass('custom-header');
    });
});
