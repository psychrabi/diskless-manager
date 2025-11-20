import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { Input } from './Input';

describe('Input', () => {
    const mockRegister = {};

    it('renders with label', () => {
        render(<Input id="test" label="Test Label" register={mockRegister} />);
        expect(screen.getByText('Test Label')).toBeInTheDocument();
    });

    it('renders without label when not provided', () => {
        render(<Input id="test" register={mockRegister} />);
        expect(screen.queryByRole('legend')).not.toBeInTheDocument();
    });

    it('shows error message when error prop provided', () => {
        render(<Input id="test" error="This field is required" register={mockRegister} />);
        expect(screen.getByRole('alert')).toHaveTextContent('This field is required');
    });

    it('does not show error message when no error', () => {
        render(<Input id="test" register={mockRegister} />);
        expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    });

    it('has aria-invalid when error exists', () => {
        render(<Input id="test" error="Error" register={mockRegister} />);
        expect(screen.getByRole('textbox')).toHaveAttribute('aria-invalid', 'true');
    });

    it('does not have aria-invalid when no error', () => {
        render(<Input id="test" register={mockRegister} />);
        expect(screen.getByRole('textbox')).toHaveAttribute('aria-invalid', 'false');
    });

    it('links error with aria-describedby', () => {
        render(<Input id="test-input" error="Error message" register={mockRegister} />);
        expect(screen.getByRole('textbox')).toHaveAttribute('aria-describedby', 'test-input-error');
    });

    it('does not have aria-describedby when no error', () => {
        render(<Input id="test" register={mockRegister} />);
        expect(screen.getByRole('textbox')).not.toHaveAttribute('aria-describedby');
    });

    it('applies placeholder', () => {
        render(<Input id="test" placeholder="Enter text" register={mockRegister} />);
        expect(screen.getByPlaceholderText('Enter text')).toBeInTheDocument();
    });

    it('is disabled when disabled prop is true', () => {
        render(<Input id="test" disabled register={mockRegister} />);
        expect(screen.getByRole('textbox')).toBeDisabled();
    });

    it('is required when required prop is true', () => {
        render(<Input id="test" required register={mockRegister} />);
        expect(screen.getByRole('textbox')).toBeRequired();
    });

    it('applies custom className', () => {
        const { container } = render(<Input id="test" className="custom-class" register={mockRegister} />);
        expect(container.querySelector('.fieldset')).toHaveClass('custom-class');
    });

    it('renders with correct input type', () => {
        render(<Input id="test" type="email" register={mockRegister} />);
        expect(screen.getByRole('textbox')).toHaveAttribute('type', 'email');
    });
});
