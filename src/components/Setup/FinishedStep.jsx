import { CheckCircle } from "lucide-react";
import { CardDescription, CardTitle } from "@/components/ui/card";
import { Button, Card } from "@/components/ui";

const FinishedStep = ({ onNavigateHome }) => {
  return (
    <Card className="border-t-4 border-emerald-600 p-12 text-center space-y-6">
      <div className="w-24 h-24 bg-emerald-600 text-white rounded-full flex items-center justify-center mx-auto shadow-2xl animate-bounce">
        <CheckCircle size={64} />
      </div>
      <div className="space-y-2">
        <CardTitle className="text-3xl font-black">All Systems Go!</CardTitle>
        <CardDescription className="max-w-md mx-auto">
          Your server environment is fully configured and ready to manage
          diskless clients.
        </CardDescription>
      </div>
      <div className="pt-4">
        <Button
          variant="primary"
          size="lg"
          className="px-12 rounded-full"
          onClick={onNavigateHome}
        >
          Go to Dashboard
        </Button>
      </div>
    </Card>
  );
};

export default FinishedStep;
